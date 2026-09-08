//! Minimal virtual pointer driver for testing: move/click/drag in logical screen coords.
use std::time::{Duration, Instant};
use wayland_client::{protocol::{wl_registry, wl_seat, wl_output}, Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{zwlr_virtual_pointer_manager_v1 as mgr, zwlr_virtual_pointer_v1 as vp};

struct State { mgr: Option<mgr::ZwlrVirtualPointerManagerV1>, seat: Option<wl_seat::WlSeat>, output: Option<wl_output::WlOutput> }

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(s: &mut Self, reg: &wl_registry::WlRegistry, ev: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = ev {
            match interface.as_str() {
                "zwlr_virtual_pointer_manager_v1" => s.mgr = Some(reg.bind(name, version.min(2), qh, ())),
                "wl_seat" => if s.seat.is_none() { s.seat = Some(reg.bind(name, version.min(7), qh, ())) },
                "wl_output" => if s.output.is_none() { s.output = Some(reg.bind(name, version.min(4), qh, ())) },
                _ => {}
            }
        }
    }
}
impl Dispatch<mgr::ZwlrVirtualPointerManagerV1, ()> for State { fn event(_: &mut Self, _: &mgr::ZwlrVirtualPointerManagerV1, _: mgr::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<vp::ZwlrVirtualPointerV1, ()> for State { fn event(_: &mut Self, _: &vp::ZwlrVirtualPointerV1, _: vp::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_seat::WlSeat, ()> for State { fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }
impl Dispatch<wl_output::WlOutput, ()> for State { fn event(_: &mut Self, _: &wl_output::WlOutput, _: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {} }

const W: u32 = 1920; const H: u32 = 1080; const BTN_LEFT: u32 = 0x110; const BTN_RIGHT: u32 = 0x111; const BTN_MIDDLE: u32 = 0x112;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let conn = Connection::connect_to_env().expect("wayland");
    let display = conn.display();
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    let _reg = display.get_registry(&qh, ());
    let mut st = State { mgr: None, seat: None, output: None };
    q.roundtrip(&mut st).unwrap();
    let mgr = st.mgr.clone().expect("no zwlr_virtual_pointer_manager_v1");
    let ptr = mgr.create_virtual_pointer_with_output(st.seat.as_ref(), st.output.as_ref(), &qh, ());
    let t0 = Instant::now();
    let now = || t0.elapsed().as_millis() as u32;
    let flush = |q: &mut wayland_client::EventQueue<State>, st: &mut State| { q.roundtrip(st).unwrap(); };
    let sleep = |ms: u64| std::thread::sleep(Duration::from_millis(ms));
    let mv = |x: f64, y: f64| { ptr.motion_absolute(now(), x.max(0.0) as u32, y.max(0.0) as u32, W, H); ptr.frame(); };
    let btn = |b: u32, down: bool| { ptr.button(now(), b, if down { wayland_client::protocol::wl_pointer::ButtonState::Pressed } else { wayland_client::protocol::wl_pointer::ButtonState::Released }); ptr.frame(); };
    let button_code = |s: &str| match s { "right" => BTN_RIGHT, "middle" => BTN_MIDDLE, _ => BTN_LEFT };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "move" => { mv(args[i+1].parse().unwrap(), args[i+2].parse().unwrap()); flush(&mut q, &mut st); i += 3; }
            "click" => { let b = if i+1 < args.len() && ["left","right","middle"].contains(&args[i+1].as_str()) { i += 1; button_code(&args[i]) } else { BTN_LEFT };
                         btn(b, true); flush(&mut q, &mut st); sleep(60); btn(b, false); flush(&mut q, &mut st); i += 1; }
            "dblclick" => { for _ in 0..2 { btn(BTN_LEFT, true); flush(&mut q, &mut st); sleep(40); btn(BTN_LEFT, false); flush(&mut q, &mut st); sleep(80); } i += 1; }
            "down" => { btn(BTN_LEFT, true); flush(&mut q, &mut st); i += 1; }
            "up" => { btn(BTN_LEFT, false); flush(&mut q, &mut st); i += 1; }
            "drag" => { let (x0, y0, x1, y1): (f64, f64, f64, f64) = (args[i+1].parse().unwrap(), args[i+2].parse().unwrap(), args[i+3].parse().unwrap(), args[i+4].parse().unwrap());
                        mv(x0, y0); flush(&mut q, &mut st); sleep(50); btn(BTN_LEFT, true); flush(&mut q, &mut st); sleep(50);
                        let n = 12; for k in 1..=n { let t = k as f64 / n as f64; mv(x0 + (x1-x0)*t, y0 + (y1-y0)*t); flush(&mut q, &mut st); sleep(16); }
                        sleep(50); btn(BTN_LEFT, false); flush(&mut q, &mut st); i += 5; }
            "sleep" => { sleep(args[i+1].parse().unwrap()); i += 2; }
            other => { eprintln!("unknown op {other}"); std::process::exit(2); }
        }
    }
    flush(&mut q, &mut st);
}

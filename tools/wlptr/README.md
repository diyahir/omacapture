# wlptr

Tiny virtual-pointer driver (wlr-virtual-pointer) for driving Omacapture in gesture tests on Hyprland without ydotool or root.

```sh
cargo build --release --manifest-path tools/wlptr/Cargo.toml
tools/wlptr/target/release/wlptr move 500 400 click
tools/wlptr/target/release/wlptr drag 200 300 600 500        # press, move in steps, release
tools/wlptr/target/release/wlptr move 500 400 click right    # also: middle, dblclick, down, up, sleep <ms>
```

Coordinates are logical screen pixels on a 1920x1080 extent; pair it with `wtype` for keys and `grim` for screenshots.

# Machine control

## Transports
| Endpoint | Typical devices | Notes |
|---|---|---|
| USB serial, 115200 | Genmitsu 3018/4030/4040, most GRBL boards | `serialport` crate, no libudev dependency |
| Wi-Fi TCP ("telnet"), port 23 | ESP32 GRBL bridges, FluidNC, grblHAL Wi-Fi | raw GRBL byte stream |
| Wi-Fi WebSocket, `ws://<ip>:81/` | ESP3D-style web-UI bridges | text frames for lines, binary for realtime bytes |
| Simulator | none | answers `ok`, tracks position, reports status — great for learning the UI |

The controller thread streams with GRBL's **character-counting** protocol (RX buffer 127 bytes by
default; grblHAL boards can use more), polls `?` at 5 Hz, and stops jobs with feed-hold +
soft-reset (the safe GRBL abort).

## Genmitsu 4040 PRO MAX + Wi-Fi module — bring-up checklist
SainSmart's wireless module/app protocol is **not publicly documented**. Plan:
1. Connect the module to your LAN; note its IP from your router.
2. In EZ-YX-craft → Machine → *Wi-Fi TCP*, try port **23**; then *WebSocket* `ws://IP:81/`.
   *Find machines* scans ports 23/81/8080 on your /24.
3. If neither answers with `Grbl…`/`ok`, capture traffic between the Genmitsu app and the module
   (phone hotspot + Wireshark, or `tcpdump` on a laptop acting as hotspot). Look for the port and
   whether it's raw GRBL text, JSON, HTTP upload-then-run, or framed binary.
4. Add `crates/ezyx-machine/src/transport/<name>.rs` implementing `Transport`
   (`write_all`, `read_some`, `describe`) and an `Endpoint` variant. If the module only supports
   "upload file then start", add an `upload_and_run` path in the controller.
5. Add a test with a recorded transcript (see `transport/sim.rs` for a fake device).

## Safety design
* The *Run job* button always shows a confirmation with time estimate and a pre-flight checklist.
* The AI assistant's machine tools (`machine_home/jog/zero/start_job`) require explicit approval
  in a modal dialog; status queries do not.
* Alarms abort streaming and show human-readable GRBL alarm text.
* `$130–$132` travel is read on connect and can update the machine profile (travel checks in
  `ops::validate`).

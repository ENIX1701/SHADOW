# SHADOW

SHADOW is the server component of AETHER.

## Overview
**Role:** Server side component  
**Platforms:** Linux  
**Idea:** Handles beacon requests and manages the implants 

## Capabilities:
- Handle multiple concurrent implants
- Send out instructions and tasks
- Manage state (monitor for "dead" ghosts)
- Gather and collect data sent out from implants

## How to run

For development environment use:

```bash
cargo run
```

This will deploy the server on `127.0.0.1` port `9999`.

## Deploy

Deployment uses a multi-stage Docker build. There are two containers in total. One is used for building the app, it has the full-fledged Rust toolchain (so cargo and rustc in this case). It's heavyweight and it's main task is to build the application. The second container is the runtime itself. It's lightweight and portable, so you can run the server in any environment supporting Docker (so a VPS, Kubernetes cluster, you own PC, or a Raspberry Pi!).



## TODO

- [ ] core server with networking
- [ ] communication protocol -> JSON over HTTP (easiest according to my research)
- [ ] maybe some database for session persistence
- [ ] GHOSTs monitoring -> ~~possibly~~ live dashboard in CHARON?

## Legal

> **Disclaimer:** This software is for educational purposes and authorized red team engagements only. The authors are not responsible for misuse.
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

```bash
# Clone the repository
git clone https://github.com/ENIX1701/SHADOW.git
cd SHADOW/      # Navigate to project folder
docker build .  # Build the Docker image

# After build is done run the container with name SHADOW
docker run -t SHADOW .
```

## Interfaces

The communication interfaces in a system as (arguably, but nonetheless!) complex as this one, with multiple modules and failure points, should be well laid-out and designed with maintenance in mind. That's why I've decided to version the RESTful API on SHADOW to make future expansion and maintenance easier (or possible at all...). The current newest version of the api is the `v1`. The documentation below will always reflect the newest version.

In the future I'd love to implement context path configuration. It'd allow the user to change the base path of the API to obfuscate the traffic further. It's in the TODO for now, but I feel like it'd be a very useful feature.

### GHOST

```markdown
| method | endpoint                       | purpose                                | notes |
|--------|--------------------------------|----------------------------------------|-------|
| POST   | /api/v1/ghost/register         | register as a new GHOST                |       |
| POST   | /api/v1/ghost/heartbeat        | main loop; send status, receive config |       |
| GET    | /api/v1/ghost/files/:id        | payload/implant download               |       |
| POST   | /api/v1/ghost/upload           | exfiltration optimization for beacon   |       |
```

### CHARON

```markdown
| method | endpoint                       | purpose                         | notes |
|--------|--------------------------------|---------------------------------|-------|
| GET    | /api/v1/charon/ghosts          | list all active GHOSTs          |       |
| GET    | /api/v1/charon/ghosts/:id      | get detailed info about a GHOST |       |
| POST   | /api/v1/charon/ghosts/:id      | update GHOST config             |       |
| POST   | /api/v1/charon/ghosts/:id/task | queue a new task for GHOST      |       |
| GET    | /api/v1/charon/ghosts/:id/tasks| get all tasks for GHOST         |       |
| GET    | /api/v1/charon/tasks/{id}      | get task details                |       |
| POST   | /api/v1/charon/ghosts/:id/kill | killswitch for a GHOST with id  |       |
```

## TODO

- [x] core server with networking
- [x] communication protocol -> JSON over HTTP (easiest according to my research)
- [ ] maybe some database for session persistence
- [x] GHOSTs monitoring -> ~~possibly~~ live dashboard in CHARON?
- [ ] [EXTRA] API path configuration (for example when building the project, as a Docker build parameter or smth)
- [ ] optimize Dockerfile -> use alpine instead of bookworm-slim for runtime
- [x] [REFACTOR] unify naming -> use GHOST in place of Implant
- [x] [REFACTOR] change status from string to enum if possible
- [ ] improve (unify) logging

## Legal

> **Disclaimer:** This software is for educational purposes and authorized red team engagements only. The authors are not responsible for misuse.

# NeoNet Demo Chat

TUI chat client for the NeoNet daemon, built with ratatui.

## Build

```bash
cargo build --release
```

## Usage

### Create a room and invite someone

```bash
neonet-demo-chat --daemon http://127.0.0.1:7780 --invite "@pubkey:localhost"
```

### Join an existing room

```bash
neonet-demo-chat --daemon http://127.0.0.1:7780 --room <room_id>
```

## Keys

- **Enter** — send message
- **Ctrl+C** — quit

## Config

- Token: `~/.neonet/session.token` (managed by the daemon)
- Pseudo: `~/.neonet-demo/config.toml` (prompted on first run)

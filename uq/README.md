# uQ integration

Files for the [uQ desktop](https://github.com/asafelobotomy/uq). They are inert without uQ:
nothing reads `/usr/share/uq/` unless uQ is installed. The package installs this folder
unconditionally (`packaging/aur/PKGBUILD`).

| Path | Installed to | What it adds |
|---|---|---|
| `widgets/scale-tail/` | `/usr/share/uq/shell/widgets/scale-tail/` | A **Tailscale** tile in quick settings (Apps group): on/off switch, the current exit node, and an exit-node picker in its detail view |

The Tailscale settings page is added separately.

## How the widget works

It runs only the commands declared in `widget.toml`, never a shell:

- `scale_tail status --json` every 15 s (5 s while the detail view is open), a small summary of
  `tailscale status --json`:

  ```json
  {"version": 1, "state": "running", "connected": true, "message": "", "tailnet": "example.com",
   "self": {"name": "laptop", "dns": "laptop.tail1234.ts.net", "ip": "100.64.0.1"},
   "exit_node": {"name": "vps", "target": "vps.tail1234.ts.net", "online": true, "active": true},
   "exit_nodes": [{"name": "vps", "target": "vps.tail1234.ts.net", "online": true, "active": true}]}
  ```

  `state` is `running`, `stopped`, `needs_login` (`message` holds the login URL), `unavailable`
  (`message` holds the error) or `other`. The command always exits 0 when it prints JSON.
- `tailscale up` / `tailscale down` for the switch.
- `tailscale set --exit-node <target>` with a `target` from `exit_nodes` (a MagicDNS name or a
  Tailscale IP, checked against `^[A-Za-z0-9][A-Za-z0-9.:-]{0,252}$`),
  `tailscale set --exit-node=auto:any` (suggested) and `tailscale set --exit-node=` (none).

Changing Tailscale without root needs `tailscale set --operator=$USER` once, as root; the tile
shows that hint when Tailscale refuses.

Under uQ the tray is not started: the autostart entry carries `NotShowIn=uQ;`, and `scale_tail`
exits at once when `org.uq.Desktop1` has an owner on the session bus (`scale_tail --tray` starts
it anyway).

## Testing

- `uqctl integrations check uq/` (from uQ 0.4): clean apart from the programs it can't find
  where scale_tail and tailscale aren't installed.
- `tests/uq/tst_widget.qml` loads the widget with a fake `ctx` and checks the tile, the detail
  view and the commands it runs:
  `QT_QPA_PLATFORM=offscreen qmltestrunner -input tests/uq/tst_widget.qml` (CI runs it).
- `cargo test` covers `status --json`, the argument parsing and the autostart entry.

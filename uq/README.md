# uQ integration

Files for the [uQ desktop](https://github.com/asafelobotomy/uq). They are inert without uQ:
nothing reads `/usr/share/uq/` unless uQ is installed. The package installs this folder
unconditionally (`packaging/aur/PKGBUILD`).

| Path | Installed to | What it adds |
|---|---|---|
| `widgets/scale-tail/` | `/usr/share/uq/shell/widgets/scale-tail/` | A **Tailscale** tile in quick settings (Apps group): on/off switch, the current exit node, and an exit-node picker in its detail view |
| `pages/scale-tail/` | `/usr/share/uq/settings/pages/scale-tail/` | A **Tailscale** page in uQ Settings (Apps): the connection switch, the account with **Sign out…**, the exit-node picker, the "use routes from other devices" and "reach the local network" switches, and the list of devices |

## How the widget works

It runs only the commands declared in `widget.toml`, never a shell:

- `scale_tail status --json` every 15 s (5 s while the detail view is open), a small summary of
  `tailscale status --json`. On uQ with `ctx.view` it polls only while quick settings is open
  (and reads once as soon as it opens); on older uQ it polls all the time:

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

## How the settings page works

It is a folder (`page.toml`, `Page.qml`, `logic.js`) and runs only the commands declared in
`page.toml`, never a shell:

- `scale_tail status --json` every 5 s while the page is shown (tailscaled sends no change
  signal and its state file is root-only, so there is nothing to watch), `tailscale status --json`
  for the devices and the account (`User[Self.UserID].LoginName`), and `tailscale debug prefs` for
  the two switches (`RouteAll`, `ExitNodeAllowLANAccess`). The devices and settings are read when
  the page opens, after each change, and when the state changes.
- `tailscale up` / `down`, `tailscale set --exit-node <target>` / `--exit-node=`,
  `--accept-routes=true|false`, `--exit-node-allow-lan-access=true|false`, and
  `tailscale logout` (uQ asks first).
- `tailscale up` is **not** run while Tailscale needs a login: it would wait for a browser until
  it timed out. The page shows the sign-in link from `status` as text to select, with a
  **Copy link** button.
- When Tailscale refuses a change with "Access denied", the page shows: "Tailscale needs you to
  be its operator once. In a terminal, run: `sudo tailscale set --operator=$USER`". Nothing runs
  as root, and uQ has no way to ask for it.
- scale_tail's "auto-connect" is not on the page: it is a tray-only marker file with no command
  line, and the tray doesn't run under uQ.
- Names, logins and links come from outside, so they are shown as plain text only (`UqLabel`, or
  a read-only text field for the sign-in link, which `logic.js` accepts only as a plain `https`
  link). At most 200 devices are listed.

## Testing

- `uqctl integrations check uq/` (uQ 0.3.0-rc.1 or later): clean apart from the programs it can't
  find where scale_tail and tailscale aren't installed. `--no-programs` skips that one check
  (CI uses it).
- `node --test 'tests/uq/*.test.mjs'` tests the page's `logic.js` and `page.toml` without Qt.
- `tests/uq/tst_page.qml` loads the page with uQ's settings page test kit (a stub
  `org.uq.settings` module and a fake `ctx`):
  `QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq` (CI runs it,
  with the widget test below).
- `tests/uq/tst_widget.qml` loads the widget with a fake `ctx` and checks the tile, the detail
  view and the commands it runs:
  `QT_QPA_PLATFORM=offscreen qmltestrunner -input tests/uq/tst_widget.qml` (CI runs it with the page test).
- `cargo test` covers `status --json`, the argument parsing and the autostart entry.

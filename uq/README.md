# uQ integration

Files for the [uQ desktop](https://github.com/asafelobotomy/uq). They are inert without uQ:
nothing reads `/usr/share/uq/` unless uQ is installed. The package installs this folder
unconditionally (`packaging/aur/PKGBUILD`).

| Path | Installed to | What it adds |
|---|---|---|
| `widgets/scale-tail/` | `/usr/share/uq/shell/widgets/scale-tail/` | A **Tailscale** tile in quick settings (Apps group): on/off switch, the current exit node, and an exit-node picker in its detail view |
| `pages/scale-tail/` | `/usr/share/uq/settings/pages/scale-tail/` | A **Tailscale** page in uQ Settings (Apps) with five tabs: connection and exit node, devices (ping, copy address), network (DNS, routes, SSH, shields up, offer as exit node), accounts, and scale_tail's own preferences |

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

## The command line behind the page

The page and the widget run only `scale_tail` and `tailscale`, never a shell. Every `scale_tail`
command below prints **one JSON document** and needs `--json` (`status` and the rest; a missing
flag exits 2). A command that fails still prints `{"ok": false, "error": "..."}` and exits 1.

| Command | What it prints or does |
|---|---|
| `status --json` | the small summary the widget reads (above) |
| `status --json --full` | that, plus `account`, every device (`peers[]`: name, ip, os, online, `exit_node_option`, `target`), Tailscale's `prefs`, `accounts[]` and scale_tail's own state (`app`) |
| `config show --json` | `{version, state, app{autostart, auto_connect, update_check}, tailscale{accept_routes, lan_access, accept_dns, shields_up, ssh, advertise_exit_node} \| null, error}` |
| `config set <key> <true\|false> --json` | changes one of those nine settings: Tailscale's through `tailscale set`, scale_tail's own through the files it keeps (`~/.config/scale_tail/autoconnect`, `no-update-check`, `~/.config/autostart/scale_tail.desktop`) |
| `exit-node <device\|none\|suggested> --json` | `tailscale set --exit-node=...` |
| `accounts --json`, `switch <id> --json` | the accounts signed in on this computer, and switching to one |
| `logout --json` | `tailscale logout` |
| `connect --json`, `disconnect --json` | `tailscale up` / `down`; `connect` is refused while a login is needed (it would wait for a browser) |
| `ping <device> --json [--count N] [--stream]` | `--stream`: one `{"event":"reply","percent":33,...}` line per reply, then `{"event":"done",...}` (progress for the page); without it, only the summary |
| `update check --json` | `{current, latest, available, url}` |
| `open` | starts the tray even under uQ (the launcher entry runs it, so "Open scale_tail" in the page works) |

`config set`, `exit-node`, `switch` and `logout` take `--dry-run`: they change nothing and print the plan uQ
Settings shows before it asks, `{summary, changes[{group, label, detail, reversible}], would_refuse, reason}`.
`would_refuse` is true when scale_tail would turn the change down (Tailscale isn't running, the exit node is
offline, the account is already in use).

## How the settings page works

It is a folder (`page.toml`, `Page.qml`, section components, `logic.js`) and says `uq_api = 3`: five tabs of cards like
Settings' own pages, streamed progress and a plan in the confirmation sheet. An older uQ skips it with "needs a newer uQ".

| Tab | Card | Reads | Changes |
|---|---|---|---|
| Connection | Tailscale | `status --full`, every 5 s while the page is shown (tailscaled sends no change signal and its state file is root-only) | `connect` / `disconnect` |
| Connection | Exit node | the same | `exit-node` (shows its plan, asks), `exit-node none`, `exit-node suggested` (plan, asks) |
| Devices | Devices | the same: system icon, online, address, search | **Copy address**, **Ping** (`ping --stream`: a progress bar, then "22 ms via DERP(nyc)"), **Use as exit node** (plan, asks) |
| Network | Names and routes, Who can reach this computer | the same | `config set` for routes and LAN access; DNS, block incoming, SSH and "offer as exit node" show their plan and ask (`config-set-careful`) |
| Account | Signed in as, Other accounts | the same | `logout` and `switch` (plan, asks) |
| Preferences | scale_tail | `config show`, followed through the three files scale_tail keeps its settings in | `config set`; **Check for updates** (`update check`) |

- Every change runs alone: while one runs, the other buttons wait, and a row shows a progress bar, then what happened.
- `connect` is never run while Tailscale needs a login: the page shows the sign-in link (plain `https` only) with **Copy link**.
- When Tailscale refuses a change with "Access denied", the page shows: "Tailscale needs you to be its operator once.
  In a terminal, run: `sudo tailscale set --operator=$USER`". Nothing runs as root, and uQ has no way to ask for it.
- The page never forces anything and every argument is checked twice: by `logic.js` before it asks uQ, and by the
  pattern in `page.toml`.
- Names, logins and links come from outside, so they are shown as plain text only (`UqLabel`, or a read-only text
  field for the sign-in link). At most 200 devices are listed.
- The tray doesn't start under uQ; the header's **Open scale_tail** starts it (for Taildrop, which stays in the app).

## Testing

- `uqctl integrations check --no-programs uq/` (CI builds it from the uQ main commit that added `uq_api = 3`, until a
  release has it): clean. `--no-programs` skips only the "program exists" check.
- `node --test 'tests/uq/*.test.mjs'` tests the page's `logic.js` and `page.toml` without Qt: every change that asks has
  a plan with the same arguments, the argument patterns equal `logic.js`'s, and the key list equals `src/cli.rs`'s.
- `QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq` loads the page with uQ's settings page
  test kit (`stub/`, `FakeContext.qml`, `tst_components.qml`: `templates/settings-page-tests/` of uQ main) and a fake
  `ctx`, and the quick-settings widget (`tst_widget.qml`). CI runs both.
- `cargo test` covers the command line: `src/cli.rs` (parsing, plans, ping) and `tests/cli.rs`, which runs the real
  binary against `tests/fake/tailscale`, a stand-in for the `tailscale` command.

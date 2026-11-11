# uQ integration tests

- `tst_widget.qml` loads the quick-settings tile (`uq/widgets/scale-tail`) with a fake `ctx`.
- `tst_page.qml` loads the Tailscale settings page (`uq/pages/scale-tail/Page.qml`) with the
  stub `org.uq.settings` module in `stub/` and `FakeContext.qml`.
- `tst_components.qml` checks the stub itself.
- `logic.test.mjs` tests `uq/pages/scale-tail/logic.js` and `page.toml` with Node (`load-logic.mjs`
  loads the logic without Qt).

`stub/`, `FakeContext.qml` and `tst_components.qml` are uQ's settings page test kit
(`templates/settings-page-tests/` in the uQ repository) as of uQ v0.3.0-rc.1. Refresh them when
moving to a newer uQ.

From the repository root:

```
node --test 'tests/uq/*.test.mjs'
QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq
```

(`/usr/lib/qt6/bin/qmltestrunner` on Arch.) Nothing under `tests/` is installed.

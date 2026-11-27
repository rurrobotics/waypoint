# AGENTS.md

Guidance for coding agents working on Waypoint, a GPUI desktop editor for
Eurobot robot paths. See `README.md` for what the app does from a user's point
of view.

## Commands

```sh
cargo build                      # debug build
cargo run -- path/to/project.json
cargo test                       # unit tests + headless UI tests (~1 s)
cargo clippy --all-targets       # must be warning-free
```

Every commit must build, pass `cargo test`, and be clippy-clean.

## Architecture

```
src/
  main.rs          app start: fonts, assets, key bindings, window
  theme.rs         neutral light/dark palette (Theme global), fonts, RADIUS
  assets.rs        embedded Lucide icons (Icon enum + gpui AssetSource)
  titlebar.rs      window frame: the toolbar doubles as the title bar; CSD controls/resize
  geom/            pure geometry, no gpui
    vec.rs         Vec2 (serialised as [x, y]), Pose, wrap_angle
    path.rs        Path, Segment(Kind): Straight | Turn | Hermite; sampling; auto turns
    shape.rs       Shape: Rect | Circle | Polygon (convex)
    footprint.rs   Robot footprint and in-place sweep radius
    collision.rs   SAT clearance vs walls/obstacles → Report
  model/           documents and files, no gpui
    map.rs         Map, Obstacle, Region, RegionColor, Origin
    project.rs     Doc (map + robot + paths, the undo unit), Project JSON file
    history.rs     snapshot undo/redo
    template.rs    built-in + user templates (project JSON without paths)
    mod.rs         config_dir()
  export/          Exporter trait; json.rs writes segments with parameters only
  ui/
    canvas.rs      View/Xform (world↔screen), handles & hit-testing, painting
    dock.rs        dock Layout (panels per side, widths, splits), persisted
    fields.rs      inline field editor (Field enum, Edit, key handling)
    widgets.rs     chip/button/icon_button/badge/tooltip helpers
    workspace.rs   root view: state, actions, key bindings, toolbar, status, render
    workspace/
      mouse.rs     canvas input: selection, pending actions, drags, previews
      inspector.rs field values (number/set_number), per-selection sections
      panels.rs    Selected / Outline / Settings panels, docks, template menu
      files.rs     open/save/export/templates (async file dialogs)
      tests.rs     headless gpui interaction tests
```

Data flow: `Workspace` owns a `Doc`. Edits go through `edit_doc` (one undo
step) or, while dragging, `begin_drag_edit`/`end_drag_edit` around in-place
mutation plus `refresh`. `changed`/`refresh` re-run `Path::fix_auto_turns`,
clamp the selection and bump `revision`; `ensure_analysis` caches sampling and
the collision report per revision. Rendering clones what the canvas needs into
a `Frame` and paints it in a gpui `canvas` element.

Interaction has **no modes**: `pending: Option<Pending>` holds a one-shot
action (draw path / shape / measure). With `None`, the canvas selects and
drags. Keep it that way; don't reintroduce tool modes.

## Conventions

- Units: millimetres and radians, angles counter-clockwise from +x, origin in a
  table corner. `Map::origin` only changes how the table is *displayed* (y up
  or down); the geometry is always the same.
- A pose is the robot's rotation centre; `theta` is the body heading. On a
  reverse segment the direction of travel is `theta + π` (`travel_heading` /
  `body_heading` in `geom/path.rs`).
- `geom` and `model` must not depend on gpui. Put tests next to the code
  (`#[cfg(test)] mod tests`).
- File formats are serde JSON. `Vec2` serialises as `[x, y]`; shapes and
  segment kinds are internally tagged (`"type"`) and flattened into their
  parent. Changing them changes the project and export formats: bump the
  version and note it as a breaking change.
- UI style: neutral zinc palette with a single blue accent (`theme.rs`), 1px
  borders, `RADIUS` corners, sentence case, Lucide icons with tooltips on
  icon-only buttons. Use `Theme` tokens, never hard-coded colours (region
  colours in `canvas::region_color` are the exception).
- Buttons in the toolbar must stop mouse-down propagation (the widgets do this
  through `pressable`), or pressing them starts a window drag.
- Commits follow Conventional Commits (`feat(scope): …`, `fix: …`,
  `refactor(…)!:` plus a `BREAKING CHANGE:` footer). Commit in small,
  functional steps.

## Testing the UI

`src/ui/workspace/tests.rs` drives the real `Workspace` on gpui's headless test
platform (dev-dependency `gpui` with `test-support`). It simulates clicks,
drags and keystrokes at table coordinates through `at()`, `click()`, `drag()`.
Add a test there for new interactions. Tests never touch the user's config:
`Layout::load/save` are no-ops under `cfg(test)` and `templates_dir` is `None`.

Visual check on Linux: GNOME blocks input injection, but screenshots work
through XWayland:

```sh
WAYLAND_DISPLAY= ./target/debug/waypoint project.json &
id=$(for w in $(xprop -root _NET_CLIENT_LIST | grep -o '0x[0-9a-f]*'); do
  xprop -id $w WM_NAME | grep -q '"Waypoint"' && echo $w; done)
import -window "$id" shot.png     # ImageMagick
```

To see a state that needs input (an open menu, the draw bar), temporarily
change the initial value in `Workspace::new`, take the screenshot, and revert.

## Gotchas

- **gpui 0.2.2 from crates.io.** APIs differ from Zed's main branch: check the
  source in `~/.cargo/registry/src/*/gpui-0.2.2` rather than guessing.
- **libc pin.** `Cargo.toml` pins `libc < 0.2.190` because gpui's
  `xattr 0.2.3` uses `libc::ENOATTR`, which newer libc removed on Linux. Drop it
  when gpui is updated.
- **Window decorations.** `main.rs` requests `WindowDecorations::Client`, so the
  toolbar is always the title bar on Linux (`titlebar.rs`). Don't drop it:
  gpui defaults to server-side decorations, and on GNOME Wayland (no
  xdg-decoration support) it still reports `Decorations::Server`, so no title
  bar would be drawn at all. Test with both `cargo run` (Wayland) and
  `WAYLAND_DISPLAY= cargo run` (X11).
- `svg()` icons don't inherit the text colour: pass it to `widgets::icon`.
- Rust 2024 `impl Trait` returns capture all lifetimes. Render helpers that
  take `&self` and `cx` return `impl IntoElement + use<>` so several can be
  built in one `render`.
- Fonts are static TTF instances (GPUI's Linux text stack handles variable
  fonts poorly), subset to Latin/Cyrillic. Their OFL licences and the Lucide
  licence must ship with releases (see `release.yml`).

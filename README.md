# Waypoint

Waypoint is a desktop editor for planning robot paths on the
[Eurobot](https://www.eurobot.org/) playing table. Draw a path out of straight
lines, turns and Hermite curves, give it your robot's size, and see the
footprint swept along the path: every spot where the robot would hit a wall or
an obstacle is highlighted. Export the result as JSON for the robot's code.

It's written in Rust with [GPUI](https://www.gpui.rs/) and runs on Linux
(Wayland and X11), Windows and macOS.

## Features

- **Paths for differential-drive robots.** A path is a start pose followed by
  segments:
  - **Straight**: drive to a point. When the robot isn't facing it, an
    *auto turn* in place is inserted and kept aligned while you edit.
  - **Turn**: rotate by an angle. Radius 0 turns in place; a larger radius
    drives an arc.
  - **Hermite**: a smooth cubic curve to a free end pose, shaped by two tangents.
  - Any segment can be driven in **reverse**.
- **Robot limits.** Set length, width, rotation-centre offset and a safety
  margin. The footprint is swept along the path (in-place turns show their
  sweep circle), collisions with walls and obstacles turn red, and the status
  bar shows the collision count and minimum clearance. Heading jumps are
  flagged.
- **The table.** Load the year's playground image (SVG, PNG or JPG) as the
  background, then draw
  obstacles and coloured regions of interest (start zones, scoring areas…).
  Rectangles, circles and convex polygons; hold **Shift** for squares.
- **Templates.** The app opens on the default template shipped with it
  (`assets/templates/default.json`), and **New** lists every shipped template
  plus your own, saved with **Save as template** (table, obstacles, regions,
  background and robot).
- Undo/redo, grid snapping, light and dark theme following the system.

## Install

Download an archive for your platform from the
[releases](https://github.com/rurrobotics/waypoint/releases) page, unpack it and run `waypoint`
(`waypoint.exe` on Windows). Open a project directly with
`waypoint strategy.json`.

## Build from source

You need a recent stable Rust toolchain (edition 2024).

On Linux, install the libraries GPUI links against first:

```sh
# Debian / Ubuntu
sudo apt install libxcb1-dev libxcb-xkb-dev libx11-xcb-dev libxkbcommon-dev \
  libxkbcommon-x11-dev libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev
# Fedora
sudo dnf install libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel \
  wayland-devel vulkan-loader-devel fontconfig-devel freetype-devel
```

Then:

```sh
cargo run --release            # start the editor
cargo run --release -- my.json # open a project
cargo test                     # geometry, model and headless UI tests
```

## Using it

| Action | How |
| --- | --- |
| New path | **New path** in the toolbar or `P`, then click to add waypoints. Pick the segment kind, Reverse and Auto turn in the floating bar. `Enter`, `Esc`, double-click or **Done** finishes. |
| Add waypoints to a path | Select the path → **Add waypoints** |
| New obstacle / region | Click a shape icon next to **Obstacle** / **Region** (`O` / `R` for a rectangle), then drag. Polygons: click the vertices, `Enter` or double-click closes. `Shift` draws a square. |
| Measure | Ruler icon or `M`, then click the start and the end (or drag). `Shift` keeps the line horizontal, vertical or diagonal (45°). |
| Edit | Drag waypoints, tangent handles (Hermite), angle and radius handles (turns) and shapes. A selected obstacle or region has handles too: rectangle corners resize (`Shift` keeps it square), a circle's edge sets the radius, polygon vertices move (staying convex), and the round knob rotates rectangles and polygons (`Shift` snaps to 15°). Or type values in the *Selected* panel: click a field, or drag its label to scrub (Shift ×10, Alt ÷10). Arrow keys step. |
| Snap | Toolbar magnet cycles the grid (off, 1, 5, 10, 50, 100 mm). `Shift` snaps angles to 15°, `Alt` disables grid snap. |
| View | Wheel zooms, right- or middle-drag pans, `F` fits the table |
| Panels | Drag a panel by its header to the other side or to reorder it, drag the edges to resize. The panel icons at the right of the toolbar (`Ctrl+B` / `Ctrl+Alt+B`) collapse the left or right side. The layout is remembered. |
| Hide | Hover a row in *Outline* and click the eye, or the eye on a group header to hide or show all paths, obstacles or regions. Hidden items stay in the project and the export, and hidden obstacles still count for collisions. |
| Delete | `Delete`/`Backspace`, the trash button in *Selected*, or hover a row in *Outline* |
| Files | `Ctrl+N` new from template, `Ctrl+O` open, `Ctrl+S` save, `Ctrl+Shift+S` save as, `Ctrl+E` export (`Cmd` on macOS) |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` (or `Ctrl+Y`) |

## Coordinates and units

Lengths are millimetres and angles radians. The origin is a table corner (set
in *Settings → Map*: bottom-left with y up, or top-left with y down), x runs
along the long side, and angles are counter-clockwise from +x. A path's pose is
the robot's **rotation centre**, and the heading is the direction the body
faces. On a reverse segment the robot drives backwards.

## Files

### Project (`*.json`)

A plain JSON file with the map, robot and paths. The background image path is
stored relative to the project when it's in the same folder tree. A template is
the same format without paths.

### Export (`Export` / `Ctrl+E`)

The paths exactly as edited: segment types and their parameters. Nothing is
sampled or derived; follow the segments from `start` to reconstruct the path.

```json
{
  "format": "waypoint-paths",
  "version": 2,
  "units": { "length": "mm", "angle": "rad" },
  "table": { "name": "Eurobot", "year": 2026, "width": 3000.0, "height": 2000.0, "origin": "bottom-left" },
  "robot": { "length": 300.0, "width": 250.0, "offset": [0.0, 0.0], "margin": 20.0 },
  "paths": [
    {
      "name": "Match A",
      "start": { "x": 250.0, "y": 250.0, "theta": 0.0 },
      "segments": [
        { "type": "straight", "to": [1000.0, 250.0] },
        { "type": "turn", "angle": 1.5708, "radius": 0.0, "auto": true },
        { "type": "turn", "angle": -1.5708, "radius": 300.0 },
        { "type": "hermite", "to": { "x": 2300.0, "y": 1450.0, "theta": 0.0 }, "t0": 900.0, "t1": 900.0 },
        { "type": "straight", "to": [1900.0, 1450.0], "reverse": true }
      ]
    }
  ]
}
```

| Segment | Parameters | Meaning |
| --- | --- | --- |
| `straight` | `to: [x, y]` | Drive straight to the point. The body faces the direction of travel (or away from it when `reverse`). |
| `turn` | `angle`, `radius` | Rotate by `angle` (CCW positive). `radius: 0` turns in place; otherwise drive an arc of that radius, turning towards the side of the angle's sign. `auto` marks turns the editor keeps aligned with the following straight. |
| `hermite` | `to: {x, y, theta}`, `t0`, `t1` | Cubic Hermite curve from the current pose to `to`. The tangents point along the travel direction at each end, with magnitudes `t0` and `t1` (mm). |

`reverse: true` appears only on segments driven backwards.

### Shipped templates

Every `.json` file in [`assets/templates/`](assets/templates) is built into the
app; `default.json` is what the app opens on and what new projects start from.
They are project files without paths. To update the default table for a new
season:

1. Draw the table in the app (background, obstacles, regions, robot) and use
   **Save as template**, or edit the JSON by hand.
2. Copy the result over `assets/templates/default.json`.
3. To ship the playground image, put it in `assets/templates/` and set
   `"background": "<image file name>"` in the template. It is embedded in the
   binary, so it works on every machine. The default template ships the
   Eurobot table as `table.svg`.

Rebuild to pick up the change; `cargo test` checks that every shipped
template parses.

### Background images

SVG, PNG and JPG work. An SVG drawn in millimetres (`viewBox="0 0 3000 2000"`)
lines up with the table exactly. SVGs are rendered in the background at 4096 px
on the long side; text in them uses the app's fonts (Inter, JetBrains Mono),
so the table looks the same on every machine.

### Where things are stored

User templates and the panel layout live in the config directory:
`~/.config/waypoint` on Linux (or `$XDG_CONFIG_HOME/waypoint`),
`~/Library/Application Support/waypoint` on macOS, and `%APPDATA%\waypoint` on
Windows.

## Releases

Pushing a tag like `v0.2.0` runs [`release.yml`](.github/workflows/release.yml).
It tests and builds Linux (x86_64, aarch64), Windows (x86_64) and macOS (Apple
Silicon, Intel), then publishes a GitHub release with archives and SHA-256
checksums. Tags containing `-` (`v0.2.0-rc1`) become prereleases.

## Credits

- Fonts: [Inter](https://rsms.me/inter/) and [JetBrains Mono](https://www.jetbrains.com/lp/mono/), SIL Open Font License (`assets/fonts`)
- Icons: [Lucide](https://lucide.dev/), ISC License (`assets/icons`)
- Table artwork: the official Eurobot 2027 playing table (`assets/templates/table.svg`), © its authors (Eurobot / Planète Sciences). It isn't covered by this project's licence.

## License

Waypoint is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [`LICENSE`](LICENSE).

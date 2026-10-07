# Screenshots

pygame games running on the Neko engine through the Python bindings
(`bindings/python`), captured headless (no window).

| File | Game | Scene |
|------|------|-------|
| `panel_rengear.png` | RENGEAR | coast, night, two-player split-screen |
| `panel_vesper.png` | VESPER | title, intro, in-game (black hole) |
| `rengear_*.png` | RENGEAR | individual frames |
| `vesper_*.png` | VESPER | individual frames |

## How they were made

```sh
# build the extension + pygame package
zig build python

# RENGEAR (its own headless render tool)
cd <rengear>
PYTHONPATH=<neko>/zig-out/python python -m tools.render_frame --track coast --z 8000 --out coast.png

# VESPER (a small headless harness that draws one scene to a PNG)
cd <vesper>
PYTHONPATH=<neko>/zig-out/python python render_frame.py play out.png
```

Set `PYTHONHASHSEED=0` on both sides if you want to compare Neko against the
real pygame with an identical (deterministic) prop layout.

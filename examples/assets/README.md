# Example assets

The examples are written to run with **no assets**: missing sprites become
labelled placeholders, sounds are skipped, and the save example creates its own
directory.

Drop files here to see the real paths in action:

```
examples/assets/
  player.png          # neko.sprite.load("player")
  logo.png            # neko.sprite.load("logo")
  font/font.ttf       # default font for neko.text / neko.sprite.text
  blip.wav            # neko.sound.load("examples/assets/blip.wav")
```

A free test font such as DejaVu Sans can be copied to `font/font.ttf`.

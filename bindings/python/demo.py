# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""A tiny pygame game running on the Neko engine.

    zig build python
    PYTHONPATH=zig-out/python python3 bindings/python/demo.py

Set ``NEKO_DEMO_FRAMES`` to run a fixed number of frames (used by the
headless smoke test).
"""

import os

import pygame


def main():
    screen = pygame.display.set_mode((320, 180))
    pygame.display.set_caption("Neko PYGAME demo")
    clock = pygame.time.Clock()
    font = pygame.font.SysFont(None, 18)

    sprite = pygame.Surface((16, 16))
    pygame.draw.circle(sprite, (255, 220, 120), (8, 8), 6)

    running = True
    frames = 0
    limit = int(os.environ.get("NEKO_DEMO_FRAMES", "0"))

    while running:
        for ev in pygame.event.get():
            if ev.type == pygame.QUIT:
                running = False
            elif ev.type == pygame.KEYDOWN and ev.key == pygame.K_ESCAPE:
                running = False

        keys = pygame.key.get_pressed()
        if keys[pygame.K_RIGHT]:
            pass

        screen.fill((12, 12, 22))
        pygame.draw.rect(screen, (200, 60, 90), (30, 30, 80, 50))
        pygame.draw.circle(screen, (80, 220, 140), (200, 100), 30)
        pygame.draw.line(screen, (90, 120, 200), (0, 0), (320, 180), 1)
        pygame.draw.polygon(screen, (240, 190, 60),
                            [(10, 150), (60, 120), (110, 160)])
        screen.blit(sprite, (150, 20))
        screen.blit(font.render("Neko + pygame!", True, (255, 255, 255)), (8, 8))

        pygame.display.flip()
        clock.tick(60)

        frames += 1
        if limit and frames >= limit:
            running = False

    pygame.quit()
    return frames


if __name__ == "__main__":
    main()

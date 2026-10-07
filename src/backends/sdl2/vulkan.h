// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Translate-c root for the `sdl2-vulkan` backend: SDL windowing/events plus
//! the Vulkan API in a single import, so `SDL_Window` and `VkSurfaceKHR` come
//! from one translation unit and types match.

#include <vulkan/vulkan.h>
#include <SDL2/SDL.h>
#include <SDL2/SDL_vulkan.h>
#include <stdio.h>

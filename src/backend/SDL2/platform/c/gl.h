// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Translate-c root for the OpenGL bindings used by the `sdl2-opengl`
//! backend. `GL_GLEXT_PROTOTYPES` makes glext.h declare the 2.0+ entry points
//! as plain functions, so they can be linked straight against libGL.

#define GL_GLEXT_PROTOTYPES 1
#include <GL/gl.h>
#include <GL/glext.h>

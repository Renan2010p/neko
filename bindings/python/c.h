/* SPDX-License-Identifier: GPL-3.0-or-later
 * Copyright (C) 2026 Renan Lucas Vieira Hilário
 *
 * Translation unit for the Python C API. Zig 0.17 removed `@cImport`, so
 * `build.zig` runs this header through `translate-c` and publishes the result
 * as the `c` module the extension imports.
 */
#include <Python.h>

# Security Policy

## Supported versions

Neko is pre-1.0. Fixes land on `main`; use the latest commit.

## Reporting a vulnerability

Please **do not** open a public issue for security problems. Instead:

- Use GitHub's private reporting: *Security → Report a vulnerability* on
  https://github.com/Renan2010p/neko, or
- email the maintainer at **renanluscad@proton.me**.

Include the affected version/commit, a description and, if possible, a minimal
reproduction. You can expect an acknowledgement within a few days.

## Scope

Neko loads assets and runs game code, and the Python bindings load a native
extension into a CPython process. The most relevant areas are the backend
implementations (`src/platform/**`), the Python extension
(`bindings/python/**`) and anything that parses external files.

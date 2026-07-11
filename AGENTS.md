# Repository guidance

ChronaHLE is an MPL-2.0 high-level emulator derived from touchHLE.

- Never add proprietary IPA files, extracted applications, Apple SDK content,
  signing keys, logs containing private data or generated build directories.
- Prefer reusable framework and runtime implementations over app-specific
  stubs.
- Preserve upstream attribution and per-file license notices.
- Keep the internal `touchHLE` crate/resource names unless a coordinated ABI
  migration explicitly requires changing them.
- Run formatting, unit tests and the relevant platform build before merging.

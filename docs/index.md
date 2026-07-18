# ChronaHLE

ChronaHLE is an experimental high-level emulator for 32-bit applications from
iPhone OS 1 through iOS 6. Windows, Linux and Android use the same Rust core.

[Download the latest release](https://github.com/MaxBetov-pdd/ChronaHLE/releases/latest)
or [view the source](https://github.com/MaxBetov-pdd/ChronaHLE).

## Current scope

- ARMv6 and ARMv7 guest applications
- Foundation, UIKit, OpenGL ES, AudioToolbox and Objective-C runtime services
- Windows x86_64, Linux x86_64 and Android ARM64 hosts
- Application-by-application compatibility work implemented as reusable APIs

ARM64 guest execution and iOS 7+ are long-term architecture goals and are not
implemented in current releases.

## Documentation

- [Supported platforms](PLATFORMS.md)
- [Application compatibility](https://appdb.chronahle.xyz/)
- [Build instructions](BUILDING.md)
- [Release checklist](RELEASE_CHECKLIST.md)

ChronaHLE contains no Apple software or commercial applications and does not
bypass DRM. Use only decrypted applications you are legally entitled to use.

ChronaHLE is independent software derived from the MPL-2.0-licensed
[touchHLE project](https://github.com/touchHLE/touchHLE). It is not affiliated
with Apple Inc.

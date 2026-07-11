# Compatibility status

Compatibility claims apply to a specific ChronaHLE build and host. A game
working on Windows does not by itself prove that graphics, audio, input and
lifecycle behavior work on Android or every Linux driver.

## Current release candidate

The July 2026 Windows candidate has exercised these locally supplied apps:

| Application | Guest | Windows result | Android result |
| --- | --- | --- | --- |
| LEGO Ninjago: The Final Battle | iOS 4-era ARMv7 | Gameplay reached in development builds; current ChronaHLE candidate ran for more than 200 seconds at about 30 FPS without a crash. Exact-candidate visual/audio checklist still needs tester sign-off. | Not tested on a physical device. |
| Angry Birds 5.2.5 | iOS 6.0 ARMv7 | Current ChronaHLE candidate ran for more than ten minutes, sustained about 60 FPS after loading, and logged no audio underruns or fatal errors. Exact-candidate menu, level, input and save-data sign-off still needs the tester. | Not tested on a physical device. |

The applications and their logs are not distributed with ChronaHLE.

## Meaning of status

- **Build verified** means compilation, packaging, signature and static binary
  checks passed. It does not mean an app was run.
- **Boots** means the app reached its first rendered screen.
- **Gameplay** means a tester reached an interactive level.
- **Release verified** means the complete checklist in
  [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) passed on the exact candidate.

No application above is marked release verified yet.

## Known limits

- Guest ARM64 is not implemented. Current guest binaries must contain an
  ARMv6 or ARMv7 slice.
- Android packaging and 16 KB page compatibility are verified, but Android
  gameplay still requires a physical ARM64 device test.
- Android now has a native OpenGL ES 2 EAGL context and a compatibility
  presentation path, but it remains unverified on physical GPU drivers. An app
  declaring the `opengles-2` device capability can still choose an ES 1.1
  context at runtime, so the declaration alone does not identify the path.
- Online services depend on both implemented iOS networking APIs and whether
  the original service still exists. Offline gameplay compatibility does not
  imply that discontinued servers can be restored.

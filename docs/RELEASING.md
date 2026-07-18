# Publishing a ChronaHLE release

Official Android releases must use the same signing key for every version.
Store the key outside the repository and back it up. Configure these GitHub
Actions secrets before creating a version tag:

- `CHRONAHLE_RELEASE_KEYSTORE_BASE64`
- `CHRONAHLE_RELEASE_STORE_PASSWORD`
- `CHRONAHLE_RELEASE_KEY_ALIAS`
- `CHRONAHLE_RELEASE_KEY_PASSWORD`

Run the full [release checklist](RELEASE_CHECKLIST.md) on the candidate commit.
Then create and push a tag matching the Cargo version:

```text
git tag -a v0.3.0 -m "ChronaHLE v0.3.0"
git push origin v0.3.0
```

The `ChronaHLE CI and Release` workflow builds Windows, Linux and Android,
checks the APK signature and 16 KB alignment, creates archives and SHA-256
checksums, and publishes them to a GitHub Release. The Android version code is
derived from the Cargo semantic version, so increase that version before every
release. Never move an existing release tag to a different commit.

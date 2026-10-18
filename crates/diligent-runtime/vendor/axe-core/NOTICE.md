# axe-core (vendored)

- Package: axe-core, licensed MPL-2.0 (see `LICENSE`, unmodified from the tarball).
- Version: 4.14.0 (published to the npm registry 2026-10-05).
- Source: https://registry.npmjs.org/axe-core/-/axe-core-4.14.0.tgz
- Retrieved: 2026-10-18.
- Tarball sha512 (matches the registry `dist.integrity`, checked before extraction):
  `sha512-9WTZxEjsZ7b13TH8JPmbV2z8CHbl80/2hm3XPEG4JgNdQLK81IBRXmSxHfMAOkSqQeRxT/0dwNDz2GOm3zzpcQ==`
- `axe.min.js` sha256: `20c09fe157a8a34a30e241aaa1fcdade657734f08ab379ecfbeb7d45cc46e878`
  (`AXE_BUNDLE_SHA256` in `src/scripts.rs`; a unit test re-hashes the file).
- Files taken from the tarball: `package/axe.min.js` and `package/LICENSE`, byte for byte. The bundle is unmodified.

The bundle runs only inside the audited page in headless Chrome and never in the engine process.
It is evaluated through the reviewed wrapper `AXE_RUN` in `src/scripts.rs`, and only when `collect --corroborate axe` is set.
To update it, replace both files from a verified tarball, then change `AXE_VERSION`, `AXE_BUNDLE_SHA256` and this notice together.

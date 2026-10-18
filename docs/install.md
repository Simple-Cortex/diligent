# Installing the native companion

Diligent's Node skill works on its own for scope, applicability, the gate and the report. The automated stages (browser capture, accessibility checks, network checks, inventory and sampling) need the companion binary `diligent-rs`. The skill never downloads anything, so installing it is your step.

The public repository `Simple-Cortex/diligent` carries the Rust source and the workflow that builds the binary, so you can either download a release asset or build it yourself. The release version equals the git tag without its leading `v`, and the workflow refuses to publish a binary whose `--version` differs from the tag. The npm package does not contain the binary or the Rust source.

## 1. Download

The release workflow in the public repository builds each platform in CI when a version tag is pushed. It attaches one archive per platform and a `SHA256SUMS` file to a draft release that a person reviews and publishes:

| Platform | Archive |
| --- | --- |
| macOS, Apple silicon | `diligent-rs-<version>-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `diligent-rs-<version>-x86_64-apple-darwin.tar.gz` |
| Linux, x86-64 (glibc) | `diligent-rs-<version>-x86_64-unknown-linux-gnu.tar.gz` |

The binary supports macOS and Linux only. Windows is not built and not supported.

## 2. Verify

Download the archive and `SHA256SUMS` into one folder, then check the archive you have:

```
shasum -a 256 -c SHA256SUMS --ignore-missing      # macOS
sha256sum -c SHA256SUMS --ignore-missing          # Linux
```

Do not run a binary whose checksum does not match. Each archive holds the binary, the project `LICENSE`, and `licenses/axe-core/` with the notice and MPL-2.0 text for the vendored axe-core bundle.

## 3. Place it

Unpack and either put `diligent-rs` on your `PATH`, or point to it:

```
tar -xzf diligent-rs-<version>-<target>.tar.gz
export DILIGENT_RS_BIN="$PWD/diligent-rs-<version>-<target>/diligent-rs"
```

Check it: `diligent-rs --version` prints the version and `diligent-rs protocol-version` prints `1`. On macOS a downloaded binary may need `xattr -d com.apple.quarantine <path>` once, because the binary is not notarized.

## 4. Chrome

Browser capture drives a local Chrome or Chromium over the DevTools Protocol. The binary finds Chrome from `--chrome-path`, then `DILIGENT_CHROME`, then common install paths. Where Chrome cannot create its sandbox (some containers), set `DILIGENT_CHROME_NO_SANDBOX=1`.

```
export DILIGENT_CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
```

## 5. What the skill does and does not do

It looks for the binary at `DILIGENT_RS_BIN`, then on `PATH`. If it is missing, the automated stages say so and stop; `init`, `status`, `gate` and `report` keep working. It never fetches, updates or installs a binary, and it never runs one whose protocol major version is not 1.

## Building from source

From a clone of the public repository, `cargo build --release -p diligent-cli --locked` produces `target/release/diligent-rs`. Rust 1.85 or newer is needed. A source build is not checksum-verified against a release, so prefer the release archive when you can.

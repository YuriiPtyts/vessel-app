# macOS download signing

## Diagnosed beta defect

The original `0.1.0-beta.1` ZIP from GitHub failed both code-signature verification and Gatekeeper assessment with:

```text
code has no resources but signature indicates they must be present
```

The main executable retained its linker-only ad-hoc signature after being placed in the app, leaving no bundle resource seal. Tauri's resource copy also flattened the frozen Python framework's relative symlinks. Signing only the outer app made `codesign --deep` pass but left an invalid Python framework signature in `Contents/Resources`; explicit verification of nested code caught it.

The packager now stages outside the workspace, copies the prepared runtime with relative symlinks intact, signs Mach-O files and framework bundles from the inside out, seals the app, and verifies every code object. It creates a ZIP without Finder/resource-fork metadata and verifies it again after normal extraction. Staging in the OS temporary directory also avoids File Provider reattaching disallowed FinderInfo during signing. The source runtime and build caches are not changed by release signing.

## Local development archive

```sh
npm run package:macos
npm run verify:macos -- .cache/releases/v0.1.0-beta.1/Vessel-0.1.0-beta.1-macos-arm64.zip
```

The versioned output directory contains the ZIP, `SHA256SUMS`, and `SIGNING.json`. This mode makes valid **ad-hoc** signatures, not an Apple-trusted release. The signing report records actual Gatekeeper acceptance; integrity checks alone are not a claim of warning-free launch. There is no quarantine-removal command or system security-policy change in the packaging process.

## Warning-free downloads on other Macs

Apple requires a **Developer ID Application** certificate with its private key and notarization. Install that identity in the build user's Keychain. Store notarization credentials in Keychain interactively with `xcrun notarytool store-credentials vessel-notary`; do not put passwords or private keys in this repository.

Then configure the packaging process:

```sh
export VESSEL_SIGNING_IDENTITY='Developer ID Application: Your Organization (TEAMID)'
export VESSEL_NOTARY_PROFILE=vessel-notary
npm run release:macos
npm run verify:macos -- .cache/releases/v0.1.0-beta.1/Vessel-0.1.0-beta.1-macos-arm64.zip --distribution
```

The distribution mode signs all bundled code with hardened runtime, retains runtime-specific entitlements (including Deno's existing JIT entitlements), submits to Apple, requires an Accepted response, staples the ticket, rebuilds the ZIP, and requires both staple validation and Gatekeeper acceptance after extraction. It fails rather than falling back to ad-hoc signing when credentials or Apple acceptance are missing.

This host currently has **zero valid code-signing identities**. The malformed signatures can be repaired here, but warning-free distribution remains blocked until the Developer ID identity and notarization profile are available. Installing a certificate is not equivalent to completing notarization; validate the complete result with the distribution check above.

## Reproduction and regression

`scripts/check-macos-package.mjs` checks the resource seal and explicitly discovers executable files/frameworks even under nonstandard resource paths. The original downloaded archive fails this command; the repaired archive passes:

```sh
node scripts/check-macos-package.mjs <downloaded-archive.zip>
```

Adding `--distribution` also requires a stapled ticket and Gatekeeper acceptance. The repaired ad-hoc archive intentionally fails that stronger check. See [verification](verification.md) for the observed results and [beta notes](beta-release.md) for remaining physical-audio checks.

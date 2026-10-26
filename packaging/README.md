# Releasing and packaging

Everything here is a template for the maintainer; none of it is built into the
program.

## Before the first release

1. Create the GitHub repo `simoncoulton/dutui` and push (`example/` is
   git-ignored: it is about 10 GB of test data).
2. Check CI is green on `master` (Linux and macOS). The Linux build has not been
   run by hand, so CI is the first real check.
3. Check the crate name is free: `cargo search dutui`.

## Cut a release

1. Bump `version` in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows,
   and commit both.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
   `.github/workflows/release.yml` refuses a tag that doesn't match the crate
   version, runs the tests, builds Linux and macOS (Intel and Apple Silicon)
   binaries, and attaches `dutui-<tag>-<target>.tar.gz` plus a `.sha256` to the
   GitHub release. Runner labels change over time; if a leg can't find its
   runner, update `os:` in that workflow.

## crates.io (`cargo install dutui`)

```
cargo publish --dry-run     # then, if happy:
cargo publish
```

Needs a crates.io token (`cargo login`). `exclude` in `Cargo.toml` keeps
`.github/`, `packaging/`, `example/` and `assets/` out of the crate.

## AUR

1. Create an AUR account and add your SSH public key to it.
2. `git clone ssh://aur@aur.archlinux.org/dutui.git aur-dutui`
   (empty for a new package).
3. Copy `packaging/aur/PKGBUILD` in, then:
   ```
   updpkgsums                          # fills in sha256sums from the v0.1.0 tag
   makepkg -f                          # builds and runs the tests
   namcap PKGBUILD dutui-*.pkg.tar.zst
   makepkg --printsrcinfo > .SRCINFO
   git add PKGBUILD .SRCINFO && git commit -m "dutui 0.1.0" && git push
   ```
4. For each later release: bump `pkgver`, reset `pkgrel=1`, `updpkgsums`,
   regenerate `.SRCINFO`, commit, push.

Optional extra packages: `dutui-git` (build from `master`) and `dutui-bin`
(install the release tarball instead of compiling).

## Homebrew (your own tap)

1. Create a GitHub repo named `homebrew-dutui`.
2. Put `packaging/homebrew/dutui.rb` at `Formula/dutui.rb` in it.
3. Fill in the checksum:
   ```
   curl -L https://github.com/simoncoulton/dutui/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
   ```
4. Test it on a Mac (or Linux with Homebrew):
   ```
   brew install --build-from-source ./Formula/dutui.rb
   brew test dutui
   brew audit --new --strict dutui
   ```
5. Users install with `brew install simoncoulton/dutui/dutui`.

For each later release, change `url` and `sha256` in the formula.

Getting into the main `homebrew-core` repository is separate: it expects a
project with some track record (stars, forks, age) and goes through review.
Most projects run their own tap until then.

#!/usr/bin/env bash
# Usage: gen-formula.sh <version> <sha-aarch64-apple-darwin> <sha-x86_64-apple-darwin> <sha-aarch64-unknown-linux-gnu> <sha-x86_64-unknown-linux-gnu>
# Writes Formula/dutui.rb pointing at the binaries attached to the GitHub release.
set -euo pipefail
version=$1
base="https://github.com/simoncoulton/dutui/releases/download/v${version}/dutui-v${version}"

cat > "$(dirname "$0")/../Formula/dutui.rb" <<RUBY
class Dutui < Formula
  desc "Fast terminal disk-usage explorer with a treemap view"
  homepage "https://github.com/simoncoulton/dutui"
  version "${version}"
  license "MIT"

  on_macos do
    on_arm do
      url "${base}-aarch64-apple-darwin.tar.gz"
      sha256 "${2}"
    end
    on_intel do
      url "${base}-x86_64-apple-darwin.tar.gz"
      sha256 "${3}"
    end
  end

  on_linux do
    on_arm do
      url "${base}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "${4}"
    end
    on_intel do
      url "${base}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${5}"
    end
  end

  def install
    bin.install "dutui"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/dutui --version")
  end
end
RUBY

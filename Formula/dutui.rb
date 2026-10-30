class Dutui < Formula
  desc "Fast terminal disk-usage explorer with a treemap view"
  homepage "https://github.com/simoncoulton/dutui"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "dutui"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/dutui --version")
  end
end

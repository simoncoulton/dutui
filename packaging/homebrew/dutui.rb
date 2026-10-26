class Dutui < Formula
  desc "Fast terminal disk-usage explorer with a treemap view"
  homepage "https://github.com/simoncoulton/dutui"
  url "https://github.com/simoncoulton/dutui/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_SHA256_OF_THE_TARBALL"
  license "MIT"
  head "https://github.com/simoncoulton/dutui.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/dutui --version")
    assert_match "Disk usage treemap", shell_output("#{bin}/dutui --help")
  end
end

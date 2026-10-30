class Dutui < Formula
  desc "Fast terminal disk-usage explorer with a treemap view"
  homepage "https://github.com/simoncoulton/dutui"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "565a64975dfec83db5be0f377d7581eda3772eb0c4949b6ef338c8e3d8cf2387"
    end
    on_intel do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "84080ed2462fbef40f54226531a1a902dee30e53ca82583aa7bec4de82abf91e"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "c8111ef2c5377bea55ee02f4f75802cc2708922be4b777642c9a96ff05aa4dd9"
    end
    on_intel do
      url "https://github.com/simoncoulton/dutui/releases/download/v0.1.0/dutui-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "17557ed216ae1c207d6855e969c4143fc9a3f709cf5ec4936642e668c8a823aa"
    end
  end

  def install
    bin.install "dutui"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/dutui --version")
  end
end

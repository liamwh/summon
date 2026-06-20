# Homebrew formula template for summon-switcher.
#
# The release workflow replaces the placeholders below before pushing this file
# into the Homebrew tap repository.

class SummonSwitcher < Formula
  desc "Tiny macOS command-line tool for opening, focusing, and cycling applications"
  homepage "https://github.com/liamwh/summon"
  license "Apache-2.0"
  version "__VERSION__"

  on_arm do
    url "https://github.com/liamwh/summon/releases/download/v__VERSION__/summon-switcher-aarch64-apple-darwin.tar.gz"
    sha256 "__ARM64_SHA256__"
  end

  on_intel do
    url "https://github.com/liamwh/summon/releases/download/v__VERSION__/summon-switcher-x86_64-apple-darwin.tar.gz"
    sha256 "__X86_64_SHA256__"
  end

  depends_on :macos

  def install
    bin.install "summon"
  end

  test do
    assert_match "summon", shell_output("#{bin}/summon --help")
  end
end

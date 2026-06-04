# Summon Homebrew formula
#
# Install from the tap:
#   brew install liamwh/tap/summon
#
# Or install directly:
#   brew install --formula https://raw.githubusercontent.com/liamwh/homebrew-tap/main/Formula/summon.rb
#
# To update after a new release, update the url and sha256 below.

class Summon < Formula
  desc "Tiny macOS command-line tool for opening, focusing, and cycling applications"
  homepage "https://github.com/liamwh/summon"
  url "https://github.com/liamwh/summon/releases/download/v0.1.0/summon-aarch64-apple-darwin.tar.gz"
  sha256 "REPLACE_WITH_ACTUAL_SHA256"
  version "0.1.0"

  depends_on :macos

  def install
    bin.install "summon"
  end

  test do
    assert_match "summon", shell_output("#{bin}/summon --help")
  end
end

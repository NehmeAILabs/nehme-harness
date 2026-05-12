class NehmeHarness < Formula
  desc "Heavy fork of zerostack — minimal coding agent in Rust, optimized for token efficiency"
  homepage "https://github.com/NehmeAILabs/nehme-harness"
  version "0.1.0-beta"
  license "GPL-3.0-only"

  on_macos do
    if Hardware::CPU.intel?
      url "https://github.com/NehmeAILabs/nehme-harness/releases/download/v0.1.0-beta/nh-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/NehmeAILabs/nehme-harness/releases/download/v0.1.0-beta/nh-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/NehmeAILabs/nehme-harness/releases/download/v0.1.0-beta/nh-x86_64-unknown-linux-musl.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/NehmeAILabs/nehme-harness/releases/download/v0.1.0-beta/nh-aarch64-unknown-linux-musl.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    # darwin tarballs contain "nh", musl tarballs contain "nh-<target>"
    bin.install Dir["nh*"].first => "nh"
  end

  test do
    assert_match(/^nehme-harness /, shell_output("#{bin}/nh --version"))
  end
end

# chimeractl (Homebrew formula template)

Tap lives in a **separate** repository: `theworker02/homebrew-chimera`.
This file is a template â€” replace `url` / `sha256` after each GitHub Release.

```ruby
class Chimeractl < Formula
  desc "Chimera enterprise mesh control-plane CLI"
  homepage "https://github.com/theworker02/chimera"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm? || Hardware::CPU.intel?
      # Universal2 artifact from release.yml (lipo merge)
      url "https://github.com/theworker02/chimera/releases/download/v#{version}/chimeractl-macos-universal2"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/theworker02/chimera/releases/download/v#{version}/chimeractl-linux-arm64"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    else
      url "https://github.com/theworker02/chimera/releases/download/v#{version}/chimeractl-linux-amd64"
      sha256 "REPLACE_WITH_RELEASE_SHA256"
    end
  end

  def install
    bin.install Dir["chimeractl*"].first => "chimeractl"
  end

  test do
    assert_match "chimeractl", shell_output("#{bin}/chimeractl --help")
  end
end
```

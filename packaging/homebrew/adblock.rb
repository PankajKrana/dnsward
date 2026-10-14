# Homebrew formula. Publish in a tap (e.g. PankajKrana/homebrew-tap) and install with:
#   brew install PankajKrana/tap/adblock
#   sudo adblock enable
class Adblock < Formula
  desc "System-wide ad and tracker blocking (filtering DNS + browser extensions)"
  homepage "https://github.com/PankajKrana/dnsward"
  url "https://github.com/PankajKrana/dnsward/archive/refs/tags/v0.1.1.tar.gz"
  sha256 "3bc776235efd8ca78f29f8655b0127fdc28e30143a7d7f6783e92c80c0e64934"
  license "MIT"
  head "https://github.com/PankajKrana/dnsward.git", branch: "main"

  depends_on "rust" => :build
  depends_on :macos

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/adblock-cli")
    bin.install_symlink "adblock" => "adblock-native-host"
    (share/"adblock/filters").install Dir["filters/*.txt"]
    system "./browser/build.sh"
    (share/"adblock").install "browser/dist/chromium", "browser/dist/firefox"
  end

  def caveats
    <<~EOS
      Turn protection on (installs the system service and routes DNS through it):
        sudo adblock enable

      Browser extensions (needed for in-page and YouTube ads):
        #{opt_share}/adblock/chromium
        #{opt_share}/adblock/firefox

      Before `brew uninstall adblock`, run:
        sudo adblock uninstall --yes
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/adblock --version")
    system bin/"adblock", "config", "validate"
  end
end

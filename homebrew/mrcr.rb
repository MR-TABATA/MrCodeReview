# Homebrew の formula。**ここは下書きの置き場で、brew はここを読まない。**
# 公開するときは MR-TABATA/homebrew-tap リポジトリの Formula/mrcr.rb に置く。
# そうすると `brew install mr-tabata/tap/mrcr` で入る。
#
# 版を上げるたびに url / sha256 / version の 3 行を Release に合わせて書き換える
# （sha256 は scripts/release.sh の出力に出る）。
#
# ビルド済みバイナリ（universal: arm64 + x86_64）を入れる形。
class Mrcr < Formula
  desc "On-demand, inspectable AI code review CLI for uncommitted changes"
  homepage "https://mrktools.app/en/codereview/"
  url "https://github.com/MR-TABATA/MrCodeReview/releases/download/v1.6.0/mrcr-1.6.0-macos.zip"
  sha256 "fe821eafb327c5902fe52bb0b1ab0652f01438d075a2fe540061afd7242f4b02"
  version "1.6.0"
  license "MIT"

  depends_on :macos => :ventura

  def install
    bin.install "mrcr"

    # シェル補完スクリプトを自動インストール
    (bash_completion/"mrcr").write Utils.safe_popen_read(bin/"mrcr", "--completions", "bash")
    (zsh_completion/"_mrcr").write Utils.safe_popen_read(bin/"mrcr", "--completions", "zsh")
    (fish_completion/"mrcr.fish").write Utils.safe_popen_read(bin/"mrcr", "--completions", "fish")
  end

  test do
    assert_match "mrcr 1.6.0", shell_output("#{bin}/mrcr --version")
    assert_match "bugs", shell_output("#{bin}/mrcr recipes")
    assert_match "compdef mrcr", (zsh_completion/"_mrcr").read
  end
end

# Releasing the terminal alpha

Ship small, versioned prereleases of `sb` and `sbt`. Keep the source auto-install loop separate from public releases: users install a chosen version and upgrade deliberately. The deprecated GUI is not part of the terminal package.

1. Land the release changes through the normal quality gate and verify CI on the exact main commit to release. The current workspace version is `0.4.0`; use `v0.4.0-alpha.1` for its first terminal prerelease. Subsequent alpha tags must be unique.
2. Create a **draft** GitHub prerelease targeting that exact commit with `gh-axi release create v0.4.0-alpha.1 --target <COMMIT> --draft --prerelease --latest=false --title 'Switchbard 0.4.0 alpha 1: terminal workspace' --body-file <NOTES.md>`. Notes should name the changes, supported platforms, install command with the exact tag, known limitations, and the issue-reporting link.
3. Manually dispatch `release-tui.yml` with `tag=v0.4.0-alpha.1`. Watch it finish for all three platforms. It checks out the release tag, builds only the terminal products, installs each actual archive in an isolated smoke test, and uploads an archive and SHA-256 file for each platform to the draft. PR builds exercise the same packaging path without publishing. A release entry alone does not prove installation works.
4. Confirm that all six archive/checksum assets exist on the draft and the installed-package smoke passes. Publish the completed prerelease with `gh-axi release edit v0.4.0-alpha.1 --draft=false`. This makes the artifacts public together. Publishing does not rebuild or replace them.
5. Download the installer from the **release tag**, run it with `--version v0.4.0-alpha.1 --bin-dir <EMPTY-DIRECTORY>`, and confirm public installation. Check both binary versions, setup, `?`, task creation, quit, and reopening using a temporary home/repository. Update the README's pinned example tag only after this passes. Keep this separate from your real task database; never point the public quickstart at an unfinished release.

The workflow supports manual dispatch with an existing draft release tag to retry a failed platform build. Re-running replaces the draft's assets for that tag; upload refuses a published release. Publish a new alpha tag for changed code.

The old `release-linux` workflow packages the deprecated GUI. It can be run manually for historical releases, but terminal alpha publication should not build it automatically.

Homebrew and npm distribution are later conveniences over the same versioned binary artifacts. Neither is advertised until its installation path is published and verified. Do not make users install a compiler, the xplan sidecar, or an agent runtime to use local tasks. Git and an interactive terminal are normal prerequisites; GitHub functionality additionally needs an authenticated `gh` installation, and agent functionality needs the relevant agent installed.

# CI/CD - Release

A release shall be built directly using the CI/CD environment GitHub Actions. The release build creates and uploads all necessary artifacts that are required for a release.

## Release branches

In order to stabilize an upcoming release or to create a patch release, a release branch can be created. The naming convention for such a branch is:

```
release-<major>.<minor>
```

For example `release-0.4`.

## Release workflow

For building a release a separate workflow exists inside `.github/workflows/release.yml`. The release workflow reuses the complete build workflow from `.github/workflows/build.yml` and its artifacts.

This allows to avoid having to duplicate the steps of the build workflow into the release workflow and thus have a single point of change for the build workflow.

The release workflow executes the build workflow, exports the build artifacts into an archive for each supported platform and uploads it to the GitHub release.

As an example the following release artifacts are created for linux-amd64:

- ankaios-linux-amd64.tar.gz
- ankaios-linux-amd64.tar.gz.sha512sum.txt

The tar.gz archive contains the pre-built binaries for the Ankaios CLI, Ankaios server and Ankaios agent. The \*.sha512sum.txt file contains the sha-512 hash of the archive.

## Release scripts

To package the desired release artifacts a separate script `tools/create_release.sh` is called inside the release job. The script calls another script `tools/create_artifacts.sh` for each platform that creates the artifacts mentioned above.

In addition, it exports the following:

- Coverage report
- ankaios.proto
- install.sh (Ankaios installation script)

Within the release workflow the build artifacts are downloaded into a temporary folder called `dist` which has the following structure:

```
├── coverage
│   ├── index.html
│   └── style.css
├── linux-amd64
│   └── bin
│       ├── ank
│       ├── ank-agent
│       └── ank-server
├── linux-arm64
│   └── bin
│       ├── ank
│       ├── ank-agent
│       └── ank-server
└── req_tracing_report.html
```

The platform specific files are downloaded into a sub-folder `dist/<os>-<platform>/bin`. Reports and shared artifacts are placed into the `dist` folder directly.

The scripts expect this folder structure to create final release artifacts.

## Adding a new Platform

If a new platform shall be supported the following steps must be done:

1. If not already done, add a build job for the new platform in `.github/workflows/build.yml` and configure the upload of the artifacts, see [CI/CD](<https://eclipse-ankaios.github.io/ankaios/main/development/ci-cd/index.md>) section.
2. Configure the release workflow under `.github/workflows/release.yml` to download the new artifacts. Under `jobs.release.steps` add a new step after the existing download steps and replace the parameters `<os>-<platform>` with the correct text (e.g. linux-amd64):

```
 jobs:
   ...
   release:
     steps:
     ...
     - name: Download artifacts for ankaios-<os>-<platform>-bin
       uses: actions/download-artifact@XXXXXXXXXX # v4.1.7
       with:
         name: ankaios-<os>-<platform>-bin
         path: dist/<os>-<platform>/bin
     ...
```

The name `ankaios-<os>-<platform>-bin` must match the used name in the upload artifact action defined inside the build workflow (`.github/workflows/build.yml`). 3. Inside `tools/create_release.sh` script add a new call to the script `tools/create_artifacts.sh` like the following:

```
...
 "${SCRIPT_DIR}"/create_artifacts.sh -p <os>-<platform>
...
```

The `<os>-<platform>` string must match the name of the sub-folder inside the dist folder. The called script expects the pre-built binaries inside `<os>-<platform>/bin`.

1. Configure the upload of the new release artifact in the release workflow inside `.github/workflows/release.yml`. Inside the step that uploads the release artifacts add the new artifact(s) to the github upload command:

```
...
run: |
  gh release upload ${{ github.ref_name }}
   ...
   <os>-<platform>/ankaios-<os>-<platform>.tar.gz \
   <os>-<platform>/ankaios-<os>-<platform>.tar.gz.sha512sum.txt
   ...
```

1. Test and run the release workflow and check if the new artifact is uploaded correctly.
2. Validate if the platform auto-detect mechanism of the installation script is supporting the new platform `tools/install.sh` and update the script if needed.

## Release notes

The release notes are generated automatically as a draft release by [Release Drafter](<https://github.com/release-drafter/release-drafter>). The procedure uses the filters for pull request labels configured inside `.github/release-drafter.yml`.

Release Drafter v7 compares Git refs rather than using a release-date cutoff. The workflow maintains a separate stable draft for `main` and each release branch using `filter-by-commitish: true`. Runs on one branch do not retarget or overwrite another branch's draft. Stable `vX.Y.Z` releases are selected by version rather than publication date; drafts and prereleases are not baselines.

On `main`, the comparison baseline is the highest published stable version, regardless of its target branch. Release Drafter calculates major, minor, or patch increments from PR labels in dry-run mode using `.github/release-drafter-preview.yml`. This small configuration extends the shared configuration but disables target filtering only for version calculation. The actual draft update uses target filtering and changes only the `main` draft.

On `release-X.Y`, the latest published stable version in the X.Y line is the baseline and the next patch version is selected automatically. Labels still categorize and exclude PRs, but cannot change the release line. A new branch with no published release in its line prepares `X.Y.0`, comparing against the highest stable version below `X.Y.0`.

For example, pending patches on `release-1.1` and `release-1.2` create independent drafts, each based on its own published release, while an enhancement on `main` can prepare a separate next-minor preview. Creating `release-1.3` prepares a new draft for that branch without transferring the preview from `main`. Both drafts may temporarily have tag/name `v1.3.0`; the target branch distinguishes them. Publish the release-branch draft, not the main preview.

Pushes update the triggering branch, and release-branch creation prepares that branch's first draft. Release publication refreshes the preview on `main`. Runs are serialized per target, so independent release branches can run in parallel. An empty comparison leaves the existing draft untouched. Multiple stable drafts on the same target cause a failure; drafts on different targets are expected. No drafts are deleted automatically.

To override the automatic choices, select `Release Drafter` in Actions, click `Run workflow`, choose the branch, and enter the optional `from` and `version` fields. The chosen branch determines the draft target. A version override is supported for the `main` preview; release branches enforce their automatically selected version. Manual baselines apply only to that run; later pushes use automatic selection. The Full Changelog link uses the same baseline as the comparison. The workflow never publishes releases. The updated workflow and configuration must be present on `main` and on release branches. A workflow on an older branch can otherwise still use the previous behavior. Publication through the GitHub UI triggers the refresh; publication using a workflow's `GITHUB_TOKEN` does not trigger another workflow.

## Preparing a release

The following steps shall be done before the actual release build is triggered.

1. Create an isssue containing tasks for getting the main branch ready:

   1. Update the versions in the project packages (Cargo.toml files) to the new version (use `tools/update_version.sh --release <new version>`).
   2. Execute tests on the supported targets.
   3. Make sure there are no security warnings of Github dependabot.
2. Finish all tasks inside the issue.
3. Build the release according to the steps described [here](<#building-a-release>).

## Building a release

Before building the release, all [preparation steps](<#preparing-a-release>) shall be finished before.

The release shall be created directly via the GitHub web frontend.

When creating a release a tag with the following naming convention must be provided: `vX.Y.Z` (e.g. v0.1.0).

1. Go to the release section inside the repository and chose the draft release that has been created by [Release Drafter](<https://github.com/release-drafter/release-drafter>).
2. Check the tag to be created on publish and adapt if required.
3. As release name enter the same tag.
4. Check the generated draft release notes based on the filter settings for pull requests inside `.github/release-drafter.yml` configuration. In case of unwanted pull requests are listed, label the pull requests correctly and generate the draft release notes again by executing the Github action `Release Drafter`.
5. Uncheck the box `Set as the latest release` (will be enabled later).
6. Click on `Publish release`.
7. Go to GitHub Actions section and wait until the release workflow has finished.
8. If the release build finished successfully, go to the release section again and validate that all required artifacts are uploaded to the new release.
9. Edit the release and enable the check box `Set as the latest release`. This setting is important otherwise the provided link for the installation script in [chapter installation](<https://eclipse-ankaios.github.io/ankaios/main/usage/installation/index.md>) is still pointing to the previous release marked as latest.
10. If the release workflow fails, delete the release and the tag manually via the GitHub web frontend. Next, check the logs of the release workflow and fix the issues. Repeat the steps starting at step 1.

> [!NOTE]
>
> There is a GitHub Action available to automatically rollback the created release and tag. This action is not used to have a better control over the cleanup procedure before a next release build is triggered. For instance, without auto-rollback a manually entered release description is still available after a failing release build.

# CI/CD - Release

A release shall be built directly using the CI/CD environment GitHub Actions.
The release build creates and uploads all necessary artifacts that are required for a release.

## Release workflow

For building a release a separate workflow exists inside `.github/workflows/release.yml`.
The release workflow reuses the complete build workflow from `.github/workflows/build.yml` and its artifacts.

This allows to avoid having to duplicate the steps of the build workflow into the release workflow
and thus have a single point of change for the build workflow.

The release workflow executes the build workflow, exports the build artifacts into an archive for each supported platform and uploads it to the GitHub release.

As an example the following release artifacts are created for linux-amd64:

- ankaios-linux-amd64.tar.gz
- ankaios-linux-amd64.tar.gz.sha512sum.txt

The tar.gz archive contains the pre-built binaries for the Ankaios CLI, Ankaios server and Ankaios agent.
The *.sha512sum.txt file contains the sha-512 hash of the archive.

## Release scripts

To package the desired release artifacts a separate script `tools/create_release.sh` is called inside the release job.
The script calls another script `tools/create_artifacts.sh` for each platform that creates the artifacts mentioned above.

In addition, it exports the following:

- Coverage report
- Requirements tracing teport
- ankaios.proto
- install.sh (Ankaios installation script)

Within the release workflow the build artifacts are downloaded into a temporary folder called `dist`
which has the following structure:

```tree
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

The platform specific files are downloaded into a sub-folder `dist/<os>-<platform>/bin`.
Reports and shared artifacts are placed into the `dist` folder directly.

The scripts expect this folder structure to create final release artifacts.

## Adding a new Platform

If a new platform shall be supported the following steps must be done:

1. If not already done, add a build job for the new platform in `.github/workflows/build.yml` and configure the upload of the artifacts, see [CI/CD](ci-cd.md) section.
2. Configure the release workflow under `.github/workflows/release.yml` to download the new artifacts.
   Under `jobs.release.steps` add a new step after the existing download steps and replace the parameters `<os>-<platform>` with the correct text (e.g. linux-amd64):

   ```tree
    jobs:
      ...
      release:
        steps:
        ...
        - name: Download artifacts for ankaios-<os>-<platform>-bin
          uses: actions/download-artifact@v3.0.2
          with:
            name: ankaios-<os>-<platform>-bin
            path: dist/<os>-<platform>/bin
        ...
   ```

   The name `ankaios-<os>-<platform>-bin` must match the used name in the upload artifact action defined inside the build workflow (`.github/workflows/build.yml`).
3. Inside `tools/create_release.sh` script add a new call to the script `tools/create_artifacts.sh` like the following:

   ```bash
   ...
    "${SCRIPT_DIR}"/create_artifacts.sh -p <os>-<platform>
   ...
   ```

   The `<os>-<platform>` string must match the name of the sub-folder inside the dist folder. The called script expects the pre-built binaries inside `<os>-<platform>/bin`.

4. Configure the upload of the new release artifact in the release workflow inside `.github/workflows/release.yml`.
   Inside the step that uploads the release artifacts add the new artifact(s) to the github upload command:

   ```bash
   ...
   run: |
     gh release upload ${{ github.ref_name }}
      ...
      <os>-<platform>/ankaios-<os>-<platform>.tar.gz \
      <os>-<platform>/ankaios-<os>-<platform>.tar.gz.sha512sum.txt
      ...
   ```

5. Test and run the release workflow and check if the new artifact is uploaded correctly.
6. Validate if the platform auto-detect mechanism of the installation script is supporting the new platform `tools/install.sh` and update the script if needed.

## Release notes

The release notes are generated automatically if a release is created via the GitHub web frontend by clicking on the `Generate release notes` button.

The procedure uses the filters for pull request labels configured inside `.github/release.yml`.

## Preparing a release

The following steps shall be done before the actual release build is triggered.

1. Create an isssue containing tasks for getting the main branch ready:
    1. Update the versions in the project packages (Cargo.toml files) to the new version.
    2. Execute tests on the supported targets.
    3. Make sure there are no security warnings of Github dependabot.
2. Finish all tasks inside the issue.
3. Build the release according to the steps described [here](#building-a-release).

## Building a release

Before building the release, all [preparation steps](#preparing-a-release) shall be finished before.

The release shall be created directly via the GitHub web frontend.

When creating a release a tag with the following naming convention must be provided: `vX.Y.Z` (e.g. v0.1.0).

1. Go to the release section inside the repository and click on `Draft a new release`.
2. Choose the tag to be created on publish.
3. As release name enter the same tag.
4. Click on the button `Generate release notes` to generate the release notes automatically based on the filter settings for pull requests inside `.github/release.yml` configuration. In case of unwanted pull requests are listed, label the pull requests correctly, delete the description field and generate the release notes again (The correction of the labels and the regeneration of the release notes can also be done after the release build.).
5. Make sure that the check box `Set as the latest release` is enabled. This setting is important otherwise the provided link for the installation script in [chapter installation](../usage/installation.md) is still pointing to the previous release marked as latest.
6. Click on `Publish release`.
7. Go to GitHub Actions section and wait until the release workflow has finished.
8. If the release build finished successfully, go to the release section again and validate that all required artifacts are uploaded to the new release.
9. If the release workflow fails, delete the release and the tag manually via the GitHub web frontend. Next, check the logs of the release workflow and fix the issues. Repeat the steps starting at step 1.

!!! Note

    There is a GitHub Action available to automatically rollback the created release and tag. This action is not used to have a better control over the cleanup procedure before a next release build is triggered. For instance, without auto-rollback a manually entered release description is still available after a failing release build.

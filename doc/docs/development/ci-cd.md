# CI/CD

As CI/CD environment GitHub Actions is used.
Merge verifications in case of opening a pull request and release builds are fully covered
into GitHub Action workflows. For information about release builds, see [CI/CD - Release](ci-cd-release.md) section.

## Merge verification

When a pull request is opened, the following pipeline jobs run:

- Linux-amd64 release build + tests in debug mode
- Linux-amd64 coverage test report
- Linux-arm64 release build (cross platform build)
- Requirements tracing

After a pull request was merged into the main branch, the jobs listed above
are executed again to validate stable branch behavior.

The steps for the build workflow are defined inside `.github/workflows/build.yml`.

The produced artifacts of the build workflow are uploaded and
can be downloaded from GitHub for debugging or testing purposes.

## Adding a new merge verification job

To add a new merge verification job adjust the workflow defined inside `.github/workflows/build.yml`.

Select a GitHub runner image matching your purposes or in case of adding a cross-build first make sure that
the build works locally within the dev container.

1. Add a new build job under the `jobs` jobs section and define a job name.
2. Add the necessary steps to the job to build the artifact(s).
3. Append a use clause to the build steps to upload the artifacts to GitHub. If a new platform build is added name the artifact according to the naming convention `ankaios-<os>-<platform>-bin` (e.g. ankaios-linux-amd64-bin) otherwise define a custom name. If the artifact is needed inside a release the artifact is referenced with this name inside the release workflow.

   ```yaml
    ...
     - uses: actions/upload-artifact@XXXXXXXXXX # v4.3.3
       with:
         name: ankaios-<os>-<platform>-bin
         path: dist/
    ...
   ```

!!! note

    GitHub Actions only runs workflow definitions from main (default) branch.
    That means when a workflow has been changed and a PR has been created for that, the
    change will not become effective before the PR is merged in main branch.
    For local testing the [act](https://github.com/nektos/act) tool can be
    used.

## Adding a new GitHub action

When introducing a new GitHub action, do not use a generic version tag (e.g. `vX` or `v.X.Y.Z`).
Specify a specific release by using it's hash and add a comment with the version tag instead. Using a generic tag might lead to an unstable Ci/CD environment, whenever the authors of the GitHub action update the generic tag to point to a newer version that contains bugs or incompatibilities with the Ankaios project.

Example:

Bad:

```yaml
...
  - uses: actions/checkout@v4
  - uses: actions/checkout@v4.1.1
...
```

Good:

```yaml
...
  - uses: actions/checkout@b4ffde65f46336ab88eb53be808477a3936bae11 # v4.1.1
...
```

## Adding GitHub action jobs

When creating a new job inside a workflow, specify a job name for each job.

Example:

```yaml
...

jobs:
  test_and_build_linux_amd64:
    name: Test and Build Linux amd64
...
```

!!! Note

    Beside being a best practice, giving a job a name is needed to reference it from the [self-service repository](https://github.com/eclipse-ankaios/.eclipsefdn) in order to configure the job as a required status check.

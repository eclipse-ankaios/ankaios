# Helper Scripts and tools

This directory contains utility scripts and folders with different usage. All of the folders contain a `README.md` file documenting their specific usage and most of the scripts support the `--help` flag for detailed usage information.

## check_copyright_headers.sh

Searches through all scripts in the project and return what files are missing the copyright header. Recommended usage via `just check-copyright-headers`.

## compare_req_tracing.py

Python script for comparing the requirement tracing report between the local branch and main. Intended usage via `just compare-requirements`.

## control_interface_workload_hash.sh

Computes the hash of the control interface tester. This is used to be able to differentiate between images.

## create_artifacts.sh

Script for creating the artifacts of Ankaios used within the `create_release.sh` script.

## create_configs.sh

Script for exporting the configs of Ankaios used within the `create_release.sh` script.

## create_release.sh

Creates the required artifacts and packages the necessary files needed for a release of Ankaios. Called automatically from the `release.yml` workflow.

## generate_docs.sh

Generates the documentation using MkDocs. Running the script with the `--help` will provide the complete commands list.

## generate_test_coverage_report.sh

Generates the test coverage report.

## install.sh

The script is used to install Ankaios. It is provided in the [installation](https://eclipse-ankaios.github.io/ankaios/latest/usage/installation/) tab of the documentation.

## run_robot_tests.sh

Used to run the system tests using the robot framework.

## setup_robot_tests.sh

Used to setup the system for an execution of the system tests. It clears the system, generates certificates, checks executables and prepares the container images needed for the tests. It is executed automatically by the robot framework at the start.

## stability_manual_test.sh

Checks the stability of Ankaios by starting, running and killing Ankaios repeatedly.

## stability_test.sh

Runs the unit tests a wapping 100 times.

## start-containerd.sh

Script used to prepare and start the containerd runtime. Run automatically at the post start of the devcontainer.

## system_specs.sh

Used to collect information about the current system. It is used in getting information that could help narrow down bugs and differences between machines.

## update_version.sh

Script is used to update the version of Ankaios in all places it can appear. It is manually run before each release.

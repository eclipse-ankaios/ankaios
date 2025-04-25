#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
ROOT_DIR="${WORKSPACE:-$(realpath -e "$SCRIPT_DIR/../")}"
DIST_DIR="${ROOT_DIR}/dist"

echo "SCRIPT_DIR: $SCRIPT_DIR"
echo "ROOT_DIR: $ROOT_DIR"

"${SCRIPT_DIR}"/create_artifacts.sh -p linux-amd64
"${SCRIPT_DIR}"/create_artifacts.sh -p linux-arm64

echo "Exporting config files"
"${SCRIPT_DIR}"/create_configs.sh

echo "Exporting coverage report"
tar -cvzf "${DIST_DIR}/"coverage-report.tar.gz --directory="${DIST_DIR}/coverage" $(ls "${DIST_DIR}/coverage")
(cd "${DIST_DIR}/coverage" && zip -r "${DIST_DIR}/"coverage-report.zip .)

echo "Exporting test results"
tar -cvzf "${DIST_DIR}/"test-results.tar.gz --directory="${DIST_DIR}/test-results" $(ls "${DIST_DIR}/test-results")
(cd "${DIST_DIR}/test-results" && zip -r "${DIST_DIR}/"test-results.zip .)

echo "Exporting control api protos"
cp "${ROOT_DIR}"/api/proto/*.proto "${DIST_DIR}"

echo "Exporting install script"
cp "${ROOT_DIR}"/tools/install.sh "${DIST_DIR}"

echo "Finished."

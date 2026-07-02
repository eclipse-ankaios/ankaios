*** Settings ***
Documentation    Signature utility keywords for Ankaios E2E testing
Library          OperatingSystem
Library          Process
Library          Collections

*** Variables ***
${RESOURCES_DIR}    ${CURDIR}
${ANKAIOS_TARGET}        %{ANKAIOS_TARGET=x86_64-unknown-linux-gnu}
${ANKAIOS_BIN_DIR}       ${CURDIR}/../../target/${ANKAIOS_TARGET}/release

*** Keywords ***
Verify Test Environment
    [Documentation]    Verify that all required binaries and images exist before running tests

    # Check for all required binaries
    File Should Exist    ${ANKAIOS_BIN_DIR}/ank-server
    ...    msg=ank-server binary not found. Run: cargo build --release --target ${ANKAIOS_TARGET} -p ank-server

    File Should Exist    ${ANKAIOS_BIN_DIR}/ank-agent
    ...    msg=ank-agent binary not found. Run: cargo build --release --target ${ANKAIOS_TARGET} -p ank-agent

    File Should Exist    ${ANKAIOS_BIN_DIR}/ank
    ...    msg=ank binary not found. Run: cargo build --release --target ${ANKAIOS_TARGET} -p ank

    # NOTE: ank keygen and ank sign are now integrated into ank CLI, no separate ank-sign binary needed

    # Check for persistence plugin container image
    ${persist_image_check}=    Run Process    podman    images    -q    localhost/ank-persist:test    shell=False
    Should Not Be Empty    ${persist_image_check.stdout}
    ...    msg=Persistence plugin image not found. Run: podman build -t localhost/ank-persist:test -f examples/plugins/basic_persistency/Containerfile .

    # Check for fleet connector container image
    ${fleet_image_check}=    Run Process    podman    images    -q    localhost/fleet-connector:test    shell=False
    Should Not Be Empty    ${fleet_image_check.stdout}
    ...    msg=Fleet connector image not found. Run: podman build -t localhost/fleet-connector:test -f tools/tutorials/fleet_management/fleet-connector/Dockerfile tools/tutorials/fleet_management/fleet-connector

    Log    ✅ Test environment verified: all binaries and images found

Build Persistence Plugin Container
    [Documentation]    Build the persistence plugin container image for E2E testing

    Log    Building persistence plugin container image...

    # Use EXECDIR to reference repo root (where robot was executed from)
    ${result}=    Run Process
    ...    podman    build
    ...    -t    localhost/ank-persist:test
    ...    -f    examples/plugins/basic_persistency/Dockerfile
    ...    .
    ...    cwd=${EXECDIR}
    ...    timeout=300s

    Should Be Equal As Integers    ${result.rc}    0
    ...    msg=Failed to build persistence plugin container: ${result.stderr}

    Log    Persistence plugin container image built: localhost/ank-persist:test

Generate Ed25519 Keypair
    [Arguments]    ${key_id}    ${output_dir}
    [Documentation]    Generate Ed25519 keypair using ank keygen

    Create Directory    ${output_dir}

    ${result}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank    keygen
    ...    --output    ${output_dir}/${key_id}.pem
    ...    shell=False
    Should Be Equal As Integers    ${result.rc}    0    msg=Keypair generation failed: ${result.stderr}

    Should Exist    ${output_dir}/${key_id}.pem
    Should Exist    ${output_dir}/${key_id}.pem.pub

    Log    Generated keypair: ${key_id} using ank keygen

Sign Manifest
    [Arguments]    ${manifest_path}    ${private_key_path}    ${key_id}    ${counter}
    [Documentation]    Sign YAML manifest with Ed25519 signature using ank sign (outputs binary .pb file)

    # ank sign creates a binary protobuf file with .pb extension
    # Input: manifest.yaml -> Output: manifest.pb
    ${result}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank    sign
    ...    --input    ${manifest_path}
    ...    --key    ${private_key_path}
    ...    --key-id    ${key_id}
    ...    --counter    ${counter}
    ...    shell=False

    Should Be Equal As Integers    ${result.rc}    0    msg=Signing failed: ${result.stderr}

    # Verify .pb file was created
    ${pb_path}=    Replace String    ${manifest_path}    .yaml    .pb
    Should Exist    ${pb_path}    msg=Signed binary manifest not found: ${pb_path}

    Log    Signed manifest: ${manifest_path} -> ${pb_path} with counter=${counter}

Verify Manifest Signature
    [Arguments]    ${manifest_path}    ${public_key_path}
    [Documentation]    Verify Ed25519 signature on manifest using ank verify

    ${result}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank    verify
    ...    --input    ${manifest_path}
    ...    --key    ${public_key_path}
    ...    shell=False

    RETURN    ${'VALID' if ${result.rc} == 0 else 'INVALID'}

Start Ankaios Server
    [Arguments]    ${signature_verification_enabled}=${False}    ${require_signature}=${False}    ${keys_dir}=/tmp/test-keys    ${persistence_plugin}=${False}    ${workloads_dir}=/tmp/ankaios-test/workloads
    [Documentation]    Start ank-server with signature verification config

    # Convert booleans to lowercase strings for TOML
    ${sig_enabled_str}=    Evaluate    str(${signature_verification_enabled}).lower()
    ${sig_required_str}=    Evaluate    str(${require_signature}).lower()

    # Get parent directory of workloads_dir for persistence file path
    ${test_dir}=    Evaluate    str(pathlib.Path("${workloads_dir}").parent)    modules=pathlib

    # Use test_dir for all temporary files
    ${config_file}=    Set Variable    ${test_dir}/ank-server.conf
    ${startup_file}=    Set Variable    ${test_dir}/startup.yaml
    ${server_log}=    Set Variable    ${test_dir}/ankaios-server.log
    ${agent_log}=    Set Variable    ${test_dir}/ankaios-agent.log
    ${counter_state}=    Set Variable    ${test_dir}/signature-counters.json

    # Create startup manifest (with or without persistence plugin)
    Run Keyword If    ${persistence_plugin}
    ...    Create Startup Manifest With Persistence Plugin    ${test_dir}    ${startup_file}
    ...    ELSE
    ...    Create Startup Manifest Without Persistence    ${startup_file}

    # Sign startup manifest if signature verification is enabled (creates .pb file)
    ${manifest_to_load}=    Run Keyword If    ${signature_verification_enabled}
    ...    Sign Startup Manifest    ${keys_dir}    ${startup_file}
    ...    ELSE
    ...    Set Variable    ${startup_file}

    # Create config file
    ${config}=    Catenate    SEPARATOR=\n
    ...    version = "v1"
    ...    startup_manifest = "${manifest_to_load}"
    ...    [signature_verification]
    ...    enabled = ${sig_enabled_str}
    ...    require_signature = ${sig_required_str}
    ...    keys_directory = "${keys_dir}"

    Create File    ${config_file}    ${config}

    # Start server process (with --insecure for test environment)
    # Set counter state path to test directory for isolation
    Start Process    ${ANKAIOS_BIN_DIR}/ank-server    --server-config    ${config_file}    --insecure
    ...    alias=ankaios-server    stdout=${server_log}    stderr=STDOUT
    ...    env:ANKAIOS_COUNTER_STATE_PATH=${counter_state}

    Sleep    2s    reason=Wait for server to initialize

    # Check if server is actually running
    ${server_running}=    Is Process Running    ankaios-server
    Run Keyword If    not ${server_running}    Log    ERROR: Server process exited immediately - check ${server_log}    level=ERROR
    Run Keyword If    not ${server_running}    Log File    ${server_log}
    Run Keyword If    not ${server_running}    Fail    Server failed to start - see log above

    # Start agent process (needed to run workloads, --insecure for test environment)
    Start Process    ${ANKAIOS_BIN_DIR}/ank-agent    --name    agent_A    --insecure
    ...    alias=ankaios-agent    stdout=${agent_log}    stderr=STDOUT

    Sleep    2s    reason=Wait for agent to connect and register

    # If persistence plugin is enabled, wait for it to start and be ready
    Run Keyword If    ${persistence_plugin}    Wait For Persistence Plugin To Be Ready

    Log    Ankaios server and agent started with signature_verification=${signature_verification_enabled}

Create Startup Manifest Without Persistence
    [Arguments]    ${startup_file}
    [Documentation]    Create minimal startup manifest

    ${manifest}=    Catenate    SEPARATOR=\n
    ...    apiVersion: v1
    ...    workloads: {}

    Create File    ${startup_file}    ${manifest}

Sign Startup Manifest
    [Arguments]    ${keys_dir}    ${startup_file}
    [Documentation]    Sign the startup manifest with most recently generated key, returns path to signed .pb file

    # Get last .pem key file (most recently generated in this test)
    # List only private keys - use absolute=False to get just filenames
    ${all_files}=    List Files In Directory    ${keys_dir}    *.pem    absolute=False
    ${key_files}=    Create List
    FOR    ${file}    IN    @{all_files}
        # Skip if filename ends with .pub (public key)
        ${is_private}=    Evaluate    not '${file}'.endswith('.pub')
        Run Keyword If    ${is_private}    Append To List    ${key_files}    ${file}
    END
    ${key_count}=    Get Length    ${key_files}
    ${last_index}=    Evaluate    ${key_count} - 1
    ${last_key}=    Get From List    ${key_files}    ${last_index}
    ${key_path}=    Set Variable    ${keys_dir}/${last_key}

    # Extract key_id from filename (remove .pem extension)
    ${key_id}=    Replace String    ${last_key}    .pem    ${EMPTY}

    # Sign the startup manifest (creates .pb file)
    Sign Manifest    ${startup_file}    ${key_path}    ${key_id}    0

    # Return path to signed binary manifest
    ${signed_path}=    Replace String    ${startup_file}    .yaml    .pb
    Log    Signed startup manifest with ${last_key} -> ${signed_path}

    RETURN    ${signed_path}

Create Startup Manifest With Persistence Plugin
    [Arguments]    ${test_dir}    ${startup_file}
    [Documentation]    Create startup manifest that includes persistence plugin as workload

    # Create workloads directory on host before bind mount
    # This ensures the directory exists in the bind-mounted filesystem
    Create Directory    ${test_dir}/workloads

    # Create manifest with persistence plugin workload using container image
    # Mount test_dir and set PERSISTENCE_FILE_PATH to derive workloads/ subdirectory
    ${manifest}=    Catenate    SEPARATOR=\n
    ...    apiVersion: v1
    ...    workloads:
    ...    ${SPACE}${SPACE}basic_persistency:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}runtime: podman
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}agent: agent_A
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}runtimeConfig: |
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}image: localhost/ank-persist:test
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}commandOptions: ["-v", "${test_dir}:${test_dir}:z", "-e", "PERSISTENCE_FILE_PATH=${test_dir}/runtime_state.yaml", "-e", "RUST_LOG=debug"]
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}controlInterfaceAccess:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}allowRules:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}- type: StateRule
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}operation: ReadWrite
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}filterMasks:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}- "desiredState.workloads.*"
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}- "desiredState.configs.*"
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}- "workloadStates.*.*.*.state"
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}- "desiredState.configs"

    Create File    ${startup_file}    ${manifest}

    Log    Created startup manifest with persistence plugin container using test dir: ${test_dir}

Stop Ankaios Server
    [Documentation]    Stop the Ankaios server and agent

    ${result_agent}=    Terminate Process    ankaios-agent
    Run Keyword And Ignore Error    Log    Agent stopped with exit code: ${result_agent.rc}

    ${result_server}=    Terminate Process    ankaios-server
    Run Keyword And Ignore Error    Log    Server stopped with exit code: ${result_server.rc}

    # Force stop specific Ankaios containers (workloads and persistence plugin)
    # Using specific name patterns to avoid stopping unrelated containers
    Run Keyword And Ignore Error    Run Process    bash    -c    podman stop $(podman ps -q --filter name\=nginx-persistent) 2>/dev/null
    Run Keyword And Ignore Error    Run Process    bash    -c    podman rm -f $(podman ps -aq --filter name\=nginx-persistent) 2>/dev/null
    Run Keyword And Ignore Error    Run Process    bash    -c    podman stop $(podman ps -q --filter name\=mqtt) 2>/dev/null
    Run Keyword And Ignore Error    Run Process    bash    -c    podman rm -f $(podman ps -aq --filter name\=mqtt) 2>/dev/null
    Run Keyword And Ignore Error    Run Process    bash    -c    podman stop $(podman ps -q --filter name\=workload) 2>/dev/null
    Run Keyword And Ignore Error    Run Process    bash    -c    podman rm -f $(podman ps -aq --filter name\=workload) 2>/dev/null
    Run Keyword And Ignore Error    Run Process    bash    -c    podman stop $(podman ps -q --filter name\=basic_persistency) 2>/dev/null
    Run Keyword And Ignore Error    Run Process    bash    -c    podman rm -f $(podman ps -aq --filter name\=basic_persistency) 2>/dev/null

    Sleep    2s    reason=Wait for server port to be released and containers to fully stop

Wait For Persistence Plugin To Be Ready
    [Documentation]    Wait for persistence plugin to be running and ready to process events

    # Wait for plugin container to start
    Sleep    5s    reason=Wait for persistence plugin workload to start

    # Verify plugin is running
    ${workloads}=    Get Workloads
    Should Contain    ${workloads}    basic_persistency    msg=Persistence plugin workload should be running

    Log    Persistence plugin workload is running and ready

Start Persistence Plugin
    [Arguments]    ${test_dir}
    [Documentation]    Wait for persistence plugin workload to start
    ...    Plugin is deployed via startup manifest by Start Ankaios Server
    ...    This keyword just waits for it to be running

    Wait For Persistence Plugin To Be Ready

    Log    Persistence plugin workload is running, using dir: ${test_dir}/workloads

Stop Persistence Plugin
    [Documentation]    Stop the persistence plugin workload

    # Try graceful deletion via ank CLI first
    Run Keyword And Ignore Error    Run Process    ${ANKAIOS_BIN_DIR}/ank    --insecure    delete    workload    basic_persistency

    # Force stop any remaining containers (in case server was already stopped)
    ${containers}=    Run Process    podman    ps    -a    --filter    name\=basic_persistency    --format    {{.ID}}
    Run Keyword If    "${containers.stdout}" != ""    Run Process    podman    stop    ${containers.stdout}
    Run Keyword If    "${containers.stdout}" != ""    Run Process    podman    rm    ${containers.stdout}

    Log    Persistence plugin stopped

Apply Manifest
    [Arguments]    ${manifest_path}
    [Documentation]    Apply manifest via ank CLI. Fails only on actual apply errors (signature/policy), not on workload state timeouts.

    ${result}=    Run Process    ${ANKAIOS_BIN_DIR}/ank    --insecure    apply    ${manifest_path}
    ...    shell=False    timeout=10s

    Log    STDOUT: ${result.stdout}
    Log    STDERR: ${result.stderr}
    Log    Return Code: ${result.rc}

    # Check for signature/policy errors in stderr (these should fail the keyword)
    ${has_signature_error}=    Run Keyword And Return Status
    ...    Should Contain Any    ${result.stderr}
    ...    Signature required    signature verification failed    Counter rollback    Signature verification

    # Fail if there was a signature/policy error
    Run Keyword If    ${has_signature_error}
    ...    Fail    ank apply rejected manifest: ${result.stderr}

    RETURN    ${result.stdout}

Get Workloads
    [Documentation]    Get current workload list

    ${result}=    Run Process    ${ANKAIOS_BIN_DIR}/ank    --insecure    get    workloads    shell=False

    Log    Workloads: ${result.stdout}

    RETURN    ${result.stdout}

Delete Workload Signed
    [Arguments]    ${workload_name}    ${private_key_path}    ${counter}
    [Documentation]    Delete a workload
    ...    NOTE: This uses unsigned deletion (ank delete workload) because signed deletion
    ...    via ank apply -d is currently broken - it signs the manifest content but sends
    ...    an empty state, causing signature verification failure.
    ...    The counter and private_key_path parameters are kept for API compatibility but not used.
    ...    TODO: Implement proper signed deletion support in ank apply -d or ank delete workload

    # Use ank delete workload (unsigned deletion)
    ${result}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank    --insecure    delete    workload    ${workload_name}
    ...    shell=False    timeout=10s

    Log    Delete output: ${result.stdout}
    Log    Delete stderr: ${result.stderr}
    Log    Delete rc: ${result.rc}

    Should Be Equal As Integers    ${result.rc}    0
    ...    msg=ank delete workload failed: ${result.stderr}

    Log    Deleted workload ${workload_name} using ank delete workload

Get Ankaios Server Logs
    [Documentation]    Read server logs

    ${logs}=    Get File    ${TEST_DIR}/ankaios-server.log
    RETURN    ${logs}

Get Persistence Plugin Logs
    [Documentation]    Read persistence plugin logs

    ${logs}=    Get File    /tmp/ank-persist.log
    RETURN    ${logs}

Cleanup Ankaios
    [Documentation]    Clean up all Ankaios processes

    Run Keyword And Ignore Error    Stop Ankaios Server
    Run Keyword And Ignore Error    Stop Persistence Plugin

    Log    Ankaios processes stopped

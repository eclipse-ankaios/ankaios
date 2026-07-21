*** Settings ***
Documentation    End-to-end tests for signature preservation through persistence plugin
Resource         ../../resources/signature_utils.resource
Library          OperatingSystem
Library          Process
Library          String
Suite Setup      Setup Test Run Directory
Suite Teardown   Run Keywords    Cleanup Ankaios    AND    Cleanup Test Run Directory

*** Variables ***
${TEST_RUN_ID}           ${EMPTY}
${KEYS_DIR}              ${EMPTY}
${TEST_DIR}              ${EMPTY}
${WORKLOADS_DIR}         ${EMPTY}
${FIXTURES_DIR}          ${CURDIR}/fixtures
${ANKAIOS_TARGET}        %{ANKAIOS_TARGET=x86_64-unknown-linux-gnu}
${ANKAIOS_BIN_DIR}       ${CURDIR}/../../../target/${ANKAIOS_TARGET}/release

*** Keywords ***
Setup Test Run Directory
    [Documentation]    Create unique test directory for this run and cleanup old containers

    # Verify environment before starting tests
    Verify Test Environment

    # Kill any stray server/agent processes from previous runs
    Run Process    pkill    -f    ank-server    shell=False
    Run Process    pkill    -f    ank-agent    shell=False
    Sleep    1s    reason=Wait for processes to die

    # Stop any existing test containers to avoid state pollution
    # Clean up persistence plugin containers
    ${result_plugin}=    Run Process    podman    ps    -a    --filter    name\=basic_persistency    --format\={{.Names}}    shell=False
    @{plugin_containers}=    Split String    ${result_plugin.stdout}    \n
    FOR    ${container}    IN    @{plugin_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    # Clean up test workload containers (nginx, mqtt, test workloads)
    ${result_workloads}=    Run Process    podman    ps    -a    --filter    name\=nginx    --format\={{.Names}}    shell=False
    @{workload_containers}=    Split String    ${result_workloads.stdout}    \n
    FOR    ${container}    IN    @{workload_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    # Clean up mqtt test containers
    ${result_mqtt}=    Run Process    podman    ps    -a    --filter    name\=mqtt    --format\={{.Names}}    shell=False
    @{mqtt_containers}=    Split String    ${result_mqtt.stdout}    \n
    FOR    ${container}    IN    @{mqtt_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    # Clean up generic test workload containers
    ${result_test}=    Run Process    podman    ps    -a    --filter    name\=workload    --format\={{.Names}}    shell=False
    @{test_containers}=    Split String    ${result_test.stdout}    \n
    FOR    ${container}    IN    @{test_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    ${timestamp}=    Evaluate    int(__import__('time').time())
    ${random}=    Evaluate    __import__('random').randint(1000, 9999)
    ${TEST_RUN_ID}=    Set Variable    ${timestamp}-${random}
    Set Suite Variable    ${TEST_RUN_ID}
    ${TEST_DIR}=    Set Variable    /tmp/ankaios-test-${TEST_RUN_ID}
    Set Suite Variable    ${TEST_DIR}
    ${KEYS_DIR}=    Set Variable    ${TEST_DIR}/keys
    Set Suite Variable    ${KEYS_DIR}
    ${WORKLOADS_DIR}=    Set Variable    ${TEST_DIR}/workloads
    Set Suite Variable    ${WORKLOADS_DIR}

    # Clean up any existing test directory from previous failed runs
    Run Keyword And Ignore Error    Remove Directory    ${TEST_DIR}    recursive=True

    Create Directory    ${TEST_DIR}
    Log    Created test directory: ${TEST_DIR}

Cleanup Test Run Directory
    [Documentation]    Remove test directory and containers after run
    # Remove persistence plugin containers
    ${result_plugin}=    Run Process    podman    ps    -a    --filter    name\=basic_persistency    --format\={{.Names}}    shell=False
    @{plugin_containers}=    Split String    ${result_plugin.stdout}    \n
    FOR    ${container}    IN    @{plugin_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    # Remove test workload containers
    ${result_workloads}=    Run Process    podman    ps    -a    --filter    name\=nginx    --format\={{.Names}}    shell=False
    @{workload_containers}=    Split String    ${result_workloads.stdout}    \n
    FOR    ${container}    IN    @{workload_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    ${result_mqtt}=    Run Process    podman    ps    -a    --filter    name\=mqtt    --format\={{.Names}}    shell=False
    @{mqtt_containers}=    Split String    ${result_mqtt.stdout}    \n
    FOR    ${container}    IN    @{mqtt_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    ${result_test}=    Run Process    podman    ps    -a    --filter    name\=workload    --format\={{.Names}}    shell=False
    @{test_containers}=    Split String    ${result_test.stdout}    \n
    FOR    ${container}    IN    @{test_containers}
        Run Keyword If    "${container}" != ""    Run Process    podman    rm    -f    ${container}    shell=False
    END

    # CRITICAL: Only remove if TEST_DIR was actually set (prevents deleting CWD on setup failure)
    Run Keyword If    "${TEST_DIR}" != ""    Run Keyword And Ignore Error    Remove Directory    ${TEST_DIR}    recursive=True

*** Test Cases ***
Signed Manifest Is Persisted With Signature Block
    [Documentation]    Verify that persistence plugin saves signed YAML with signature intact
    [Tags]    signature    persistence    critical

    # Setup: Generate test keypair
    Generate Ed25519 Keypair    test-key-001    ${KEYS_DIR}

    # Copy template and sign it (creates binary .pb file)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/test_manifest.yaml
    Sign Manifest    /tmp/test_manifest.yaml    ${KEYS_DIR}/test-key-001.pem    test-key-001    1

    # Start server with signature verification enabled
    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    # Apply signed manifest (use .pb file created by ank sign)
    ${result}=    Apply Manifest    /tmp/test_manifest.pb
    Log    Apply result: ${result}

    # Wait for persistence
    Sleep    3s    reason=Wait for workload to reach Running state and persist

    # Verify workload persisted as binary protobuf (.pb) file
    File Should Exist    ${WORKLOADS_DIR}/nginx-persistent.pb
    ...    msg=Workload file should exist in workloads directory as .pb

    # Verify signature is valid using ank verify
    ${result}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/nginx-persistent.pb
    ...    --key    ${KEYS_DIR}/test-key-001.pem.pub
    ...    shell=False

    Should Be Equal As Integers    ${result.rc}    0
    ...    msg=Signature verification should succeed (exit code 0)
    Log    ank verify output: ${result.stdout}
    Log    ank verify stderr: ${result.stderr}

    Log    ✅ SUCCESS: Persisted .pb file contains valid signature

    [Teardown]    Run Keywords
    ...    Stop Ankaios Server
    ...    AND    Remove File    /tmp/test_manifest.yaml
    ...    AND    Remove File    /tmp/test_manifest.pb
    ...    AND    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True

Server Restart Restores Signed State Successfully
    [Documentation]    Verify complete signature chain through restart
    [Tags]    signature    persistence    restart    critical

    # Ensure clean state (in case previous test didn't clean up properly)
    Run Keyword And Ignore Error    Terminate Process    ankaios-agent
    Run Keyword And Ignore Error    Terminate Process    ankaios-server
    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True
    Create Directory    ${WORKLOADS_DIR}
    Sleep    2s    reason=Ensure previous test processes are dead

    # Setup
    Generate Ed25519 Keypair    test-key-002    ${KEYS_DIR}
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/test_manifest2.yaml
    Sign Manifest    /tmp/test_manifest2.yaml    ${KEYS_DIR}/test-key-002.pem    test-key-002    10

    # Initial application
    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    ${result}=    Apply Manifest    /tmp/test_manifest2.pb
    Log    Initial apply: ${result}

    Sleep    3s    reason=Wait for persistence

    # Verify workload is running
    ${workloads}=    Get Workloads
    Should Contain    ${workloads}    nginx-persistent
    ...    msg=Workload should be present before restart

    # Verify workload file was created as .pb
    File Should Exist    ${WORKLOADS_DIR}/nginx-persistent.pb
    ...    msg=Workload file should be created in workloads directory as .pb

    # Restart server (simulates reboot)
    Stop Ankaios Server
    Sleep    2s    reason=Ensure clean shutdown

    # Start fresh server + plugin
    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    Sleep    3s    reason=Wait for state restoration

    # Verify workload was restored
    ${workloads_after}=    Get Workloads
    Should Contain    ${workloads_after}    nginx-persistent
    ...    msg=Workload should be restored after restart

    # Verify server logs show signature verification on restore
    ${logs}=    Get Ankaios Server Logs
    Should Contain    ${logs}    signature verified
    ...    msg=Server should verify signature during restoration

    Log    ✅ SUCCESS: Workload restored successfully with signature verification

    [Teardown]    Run Keywords
    ...    Stop Ankaios Server
    ...    AND    Remove File    /tmp/test_manifest2.yaml
    ...    AND    Remove File    /tmp/test_manifest2.pb
    ...    AND    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True

Tampered Persistence File Is Rejected On Restore
    [Documentation]    Verify signature chain detects tampering
    [Tags]    signature    security    tampering    critical

    # Clean state from previous tests
    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True
    Create Directory    ${WORKLOADS_DIR}

    # Setup
    Generate Ed25519 Keypair    test-key-003    ${KEYS_DIR}
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/test_manifest3.yaml
    Sign Manifest    /tmp/test_manifest3.yaml    ${KEYS_DIR}/test-key-003.pem    test-key-003    20

    # Initial setup
    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    Apply Manifest    /tmp/test_manifest3.pb
    Sleep    3s    reason=Wait for persistence

    # Verify workload exists
    ${workloads_before}=    Get Workloads
    Should Contain    ${workloads_before}    nginx-persistent

    # Stop server
    Stop Ankaios Server
    Sleep    1s

    # TAMPER: Corrupt the signature bytes within the protobuf structure
    # This keeps the protobuf valid (can be decoded) but makes signature verification fail
    ${result}=    Run Process    python3    ${CURDIR}/tamper_signature.py    ${WORKLOADS_DIR}/nginx-persistent.pb
    Should Be Equal As Integers    ${result.rc}    0    msg=Tampering script failed: ${result.stderr}
    Log    ⚠️ Tampered signature in workload file (protobuf structure remains valid)

    # Try to restart with tampered file
    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    Sleep    3s    reason=Wait for restoration attempt

    # Verify server rejected tampered state
    ${logs}=    Get Ankaios Server Logs
    Should Contain    ${logs}    signature verification failed
    ...    msg=Server should reject tampered persistence file

    # Verify workload was NOT restored
    ${workloads_after}=    Get Workloads
    Should Not Contain    ${workloads_after}    nginx-persistent
    ...    msg=Tampered workload should not be restored

    Log    ✅ SUCCESS: Tampered persistence file was rejected

    [Teardown]    Run Keywords
    ...    Stop Ankaios Server
    ...    AND    Remove File    /tmp/test_manifest3.yaml
    ...    AND    Remove File    /tmp/test_manifest3.pb
    ...    AND    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True

Unsigned Manifest Is Rejected When Require Signature Is True
    [Documentation]    Verify policy enforcement for unsigned manifests
    [Tags]    signature    policy    security

    # Clean state from previous tests
    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True
    Create Directory    ${WORKLOADS_DIR}

    # Setup
    Generate Ed25519 Keypair    test-key-004    ${KEYS_DIR}

    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    # Try to apply unsigned manifest
    Copy File    ${FIXTURES_DIR}/unsigned_workload.yaml    /tmp/unsigned_test.yaml

    ${result}=    Run Keyword And Ignore Error
    ...    Apply Manifest    /tmp/unsigned_test.yaml

    # Verify rejection
    ${status}    ${output}=    Set Variable    ${result}

    Should Be Equal    ${status}    FAIL
    ...    msg=Unsigned manifest should be rejected

    # Check logs for signature requirement error
    ${logs}=    Get Ankaios Server Logs
    Should Contain    ${logs}    Signature required
    ...    msg=Logs should indicate signature requirement

    Log    ✅ SUCCESS: Unsigned manifest was rejected as expected

    [Teardown]    Run Keywords
    ...    Stop Ankaios Server
    ...    AND    Remove File    /tmp/unsigned_test.yaml
    ...    AND    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True

Counter Rollback Attack Is Prevented
    [Documentation]    Verify monotonic counter enforcement prevents replay attacks
    [Tags]    signature    security    counter    critical

    # Clean state from previous tests
    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True
    Create Directory    ${WORKLOADS_DIR}

    # Setup
    Generate Ed25519 Keypair    test-key-005    ${KEYS_DIR}

    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    # Apply manifest with counter=50
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/test_counter.yaml
    Sign Manifest    /tmp/test_counter.yaml    ${KEYS_DIR}/test-key-005.pem    test-key-005    50

    ${result1}=    Apply Manifest    /tmp/test_counter.pb
    Log    Applied with counter=50: ${result1}
    Sleep    1s

    # Apply manifest with counter=51 (should succeed)
    Sign Manifest    /tmp/test_counter.yaml    ${KEYS_DIR}/test-key-005.pem    test-key-005    51

    ${result2}=    Apply Manifest    /tmp/test_counter.pb
    Should Not Contain    ${result2}    error
    ...    msg=Counter=51 should be accepted after counter=50
    Sleep    1s

    # Try to apply manifest with counter=49 (rollback attempt)
    Sign Manifest    /tmp/test_counter.yaml    ${KEYS_DIR}/test-key-005.pem    test-key-005    49

    ${rollback_result}=    Run Keyword And Ignore Error
    ...    Apply Manifest    /tmp/test_counter.pb

    ${status}    ${output}=    Set Variable    ${rollback_result}

    Should Be Equal    ${status}    FAIL
    ...    msg=Counter rollback should be rejected

    # Verify logs show counter rollback detection
    ${logs}=    Get Ankaios Server Logs
    Should Contain    ${logs}    Counter rollback
    ...    msg=Logs should indicate counter rollback attempt

    Log    ✅ SUCCESS: Counter rollback attack was prevented

    [Teardown]    Run Keywords
    ...    Stop Ankaios Server
    ...    AND    Remove File    /tmp/test_counter.yaml
    ...    AND    Remove File    /tmp/test_counter.pb
    ...    AND    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True

Multiple Workloads Persist To Separate Files Without Overwriting
    [Documentation]    Verify per-workload file persistence prevents workload loss
    ...    This test reproduces the production bug where mqtt_test overwrote mqtt_fedora.
    ...    With per-workload files, each workload gets its own .pb file (binary protobuf) with complete signature.
    [Tags]    signature    persistence    multi-workload    critical

    # Clean state from previous tests
    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True
    Create Directory    ${WORKLOADS_DIR}

    # Setup
    Generate Ed25519 Keypair    test-key-006    ${KEYS_DIR}

    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    # Start persistence plugin

    # Create and apply first workload (mqtt_fedora equivalent)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/workload1.yaml
    # Modify to create mqtt_fedora workload
    ${content1}=    Get File    /tmp/workload1.yaml
    ${modified1}=    Replace String    ${content1}    nginx-persistent    mqtt_fedora
    Create File    /tmp/workload1.yaml    ${modified1}
    Sign Manifest    /tmp/workload1.yaml    ${KEYS_DIR}/test-key-006.pem    test-key-006    1778665427

    ${result1}=    Apply Manifest    /tmp/workload1.pb
    Log    Applied mqtt_fedora: ${result1}
    Sleep    3s    reason=Wait for persistence

    # Verify mqtt_fedora was persisted to its own .pb file
    File Should Exist    ${WORKLOADS_DIR}/mqtt_fedora.pb
    ...    msg=mqtt_fedora should be persisted to separate .pb file

    # Verify signature is valid
    ${verify_result}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/mqtt_fedora.pb
    ...    --key    ${KEYS_DIR}/test-key-006.pem.pub
    ...    shell=False

    Should Be Equal As Integers    ${verify_result.rc}    0
    ...    msg=mqtt_fedora signature verification should succeed

    # Create and apply second workload (mqtt_test equivalent)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/workload2.yaml
    # Modify to create mqtt_test workload
    ${content2}=    Get File    /tmp/workload2.yaml
    ${modified2}=    Replace String    ${content2}    nginx-persistent    mqtt_test
    Create File    /tmp/workload2.yaml    ${modified2}
    Sign Manifest    /tmp/workload2.yaml    ${KEYS_DIR}/test-key-006.pem    test-key-006    1778665428

    ${result2}=    Apply Manifest    /tmp/workload2.pb
    Log    Applied mqtt_test: ${result2}
    Sleep    3s    reason=Wait for persistence

    # CRITICAL VERIFICATION: Both workload .pb files must exist
    File Should Exist    ${WORKLOADS_DIR}/mqtt_fedora.pb
    ...    msg=mqtt_fedora.pb should NOT be overwritten by mqtt_test
    File Should Exist    ${WORKLOADS_DIR}/mqtt_test.pb
    ...    msg=mqtt_test should be persisted to separate .pb file

    # Verify both signatures are still valid
    ${verify_fedora}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/mqtt_fedora.pb
    ...    --key    ${KEYS_DIR}/test-key-006.pem.pub
    ...    shell=False

    Should Be Equal As Integers    ${verify_fedora.rc}    0
    ...    msg=mqtt_fedora signature should still be valid

    ${verify_test}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/mqtt_test.pb
    ...    --key    ${KEYS_DIR}/test-key-006.pem.pub
    ...    shell=False

    Should Be Equal As Integers    ${verify_test.rc}    0
    ...    msg=mqtt_test signature should be valid

    # Verify both workloads are running
    ${workloads}=    Get Workloads
    Should Contain    ${workloads}    mqtt_fedora
    ...    msg=mqtt_fedora should be running
    Should Contain    ${workloads}    mqtt_test
    ...    msg=mqtt_test should be running

    # Test server restart - both workloads should restore
    # Check what files exist before restart
    ${files_before}=    Run Process    ls    -la    ${WORKLOADS_DIR}    shell=False
    Log    Files before restart: ${files_before.stdout}

    Stop Ankaios Server
    Sleep    2s

    # Verify files still exist after server stop
    ${files_after_stop}=    Run Process    ls    -la    ${WORKLOADS_DIR}    shell=False
    Log    Files after stop: ${files_after_stop.stdout}

    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    Sleep    3s    reason=Wait for restoration

    # Preserve logs for debugging
    Run Process    cp    ${TEST_DIR}/ankaios-server.log    /tmp/test6-restart.log    shell=False

    # Check plugin logs
    ${plugin_container}=    Run Process    podman    ps    -q    --filter    name\=basic_persistency    shell=False
    ${plugin_logs}=    Run Process    podman    logs    --tail    100    ${plugin_container.stdout.strip()}    shell=False
    Log    Plugin logs after restart: ${plugin_logs.stdout}

    # Verify both workloads restored from separate files
    ${workloads_after}=    Get Workloads
    Log    Workloads after restart: ${workloads_after}
    Should Contain    ${workloads_after}    mqtt_fedora
    ...    msg=mqtt_fedora should be restored after restart
    Should Contain    ${workloads_after}    mqtt_test
    ...    msg=mqtt_test should be restored after restart

    # Verify signatures were validated on restore
    ${logs}=    Get Ankaios Server Logs
    Should Contain    ${logs}    signature verified
    ...    msg=Server should verify signatures during restoration

    Log    ✅ SUCCESS: Multiple workloads persisted to separate files, no overwriting occurred

    [Teardown]    Run Keywords
    ...    Stop Ankaios Server
    ...    AND    Remove File    /tmp/workload1.yaml
    ...    AND    Remove File    /tmp/workload2.yaml
    ...    AND    Remove Directory    ${WORKLOADS_DIR}    recursive=True

Complete Workload Lifecycle With Updates And Deletions
    [Documentation]    End-to-end test covering full lifecycle: add, update, delete with signature verification
    ...    Tests the complete flow:
    ...    1. Add multiple signed workloads
    ...    2. Update workloads with new signatures (higher counters)
    ...    3. Delete workloads and verify file cleanup
    ...    4. Verify persistence through server restart
    [Tags]    signature    persistence    lifecycle    critical

    # Clean state from previous tests
    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True
    Create Directory    ${WORKLOADS_DIR}

    # Setup
    Generate Ed25519 Keypair    test-key-007    ${KEYS_DIR}

    # Start server with signature verification enabled but not required
    # This allows signed workload additions/updates AND unsigned deletions
    # NOTE: require_signature=False is needed because ank delete workload doesn't support signatures
    # and signed deletion via ank apply -d is currently broken (signs manifest but sends empty state)
    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${False}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    # Phase 1: Add multiple workloads
    Log    === PHASE 1: Adding multiple workloads ===

    # Add workload A (counter=100)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/workload_a.yaml
    ${content_a}=    Get File    /tmp/workload_a.yaml
    ${modified_a}=    Replace String    ${content_a}    nginx-persistent    workload_a
    Create File    /tmp/workload_a.yaml    ${modified_a}
    Sign Manifest    /tmp/workload_a.yaml    ${KEYS_DIR}/test-key-007.pem    test-key-007    100

    Apply Manifest    /tmp/workload_a.pb
    Sleep    2s

    # Add workload B (counter=101)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/workload_b.yaml
    ${content_b}=    Get File    /tmp/workload_b.yaml
    ${modified_b}=    Replace String    ${content_b}    nginx-persistent    workload_b
    Create File    /tmp/workload_b.yaml    ${modified_b}
    Sign Manifest    /tmp/workload_b.yaml    ${KEYS_DIR}/test-key-007.pem    test-key-007    101

    Apply Manifest    /tmp/workload_b.pb
    Sleep    2s

    # Add workload C (counter=102)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/workload_c.yaml
    ${content_c}=    Get File    /tmp/workload_c.yaml
    ${modified_c}=    Replace String    ${content_c}    nginx-persistent    workload_c
    Create File    /tmp/workload_c.yaml    ${modified_c}
    Sign Manifest    /tmp/workload_c.yaml    ${KEYS_DIR}/test-key-007.pem    test-key-007    102

    Apply Manifest    /tmp/workload_c.pb
    Sleep    2s

    # Verify all three workload files exist as .pb (binary protobuf)
    File Should Exist    ${WORKLOADS_DIR}/workload_a.pb
    File Should Exist    ${WORKLOADS_DIR}/workload_b.pb
    File Should Exist    ${WORKLOADS_DIR}/workload_c.pb

    # Verify signatures using ank verify (cannot parse binary .pb files as YAML)
    ${verify_a}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/workload_a.pb
    ...    --key    ${KEYS_DIR}/test-key-007.pem.pub
    ...    shell=False
    Should Be Equal As Integers    ${verify_a.rc}    0
    ...    msg=workload_a signature verification should succeed

    ${verify_b}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/workload_b.pb
    ...    --key    ${KEYS_DIR}/test-key-007.pem.pub
    ...    shell=False
    Should Be Equal As Integers    ${verify_b.rc}    0
    ...    msg=workload_b signature verification should succeed

    ${verify_c}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/workload_c.pb
    ...    --key    ${KEYS_DIR}/test-key-007.pem.pub
    ...    shell=False
    Should Be Equal As Integers    ${verify_c.rc}    0
    ...    msg=workload_c signature verification should succeed

    Log    ✅ Phase 1 complete: 3 workloads persisted as .pb files with valid signatures

    # Phase 2: Update workloads with new signatures
    Log    === PHASE 2: Updating workloads with new signatures ===

    # Update workload A (counter 100 → 200)
    Sign Manifest    /tmp/workload_a.yaml    ${KEYS_DIR}/test-key-007.pem    test-key-007    200
    Apply Manifest    /tmp/workload_a.pb
    Sleep    2s

    # Verify workload A file updated - signature still valid (cannot check counter in binary .pb)
    ${verify_a_updated}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/workload_a.pb
    ...    --key    ${KEYS_DIR}/test-key-007.pem.pub
    ...    shell=False
    Should Be Equal As Integers    ${verify_a_updated.rc}    0
    ...    msg=Updated workload_a signature should be valid

    # Update workload B (counter 101 → 201)
    Sign Manifest    /tmp/workload_b.yaml    ${KEYS_DIR}/test-key-007.pem    test-key-007    201
    Apply Manifest    /tmp/workload_b.pb
    Sleep    2s

    # Verify workload B file updated
    ${verify_b_updated}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/workload_b.pb
    ...    --key    ${KEYS_DIR}/test-key-007.pem.pub
    ...    shell=False
    Should Be Equal As Integers    ${verify_b_updated.rc}    0
    ...    msg=Updated workload_b signature should be valid

    # Verify workload C unchanged (signature still valid)
    ${verify_c_unchanged}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/workload_c.pb
    ...    --key    ${KEYS_DIR}/test-key-007.pem.pub
    ...    shell=False
    Should Be Equal As Integers    ${verify_c_unchanged.rc}    0
    ...    msg=Unchanged workload_c signature should still be valid

    # Verify all three .pb files still exist
    File Should Exist    ${WORKLOADS_DIR}/workload_a.pb
    File Should Exist    ${WORKLOADS_DIR}/workload_b.pb
    File Should Exist    ${WORKLOADS_DIR}/workload_c.pb

    Log    ✅ Phase 2 complete: Workloads A and B updated with new signatures

    # Phase 3: Delete workloads and verify file cleanup
    Log    === PHASE 3: Deleting workloads ===

    # Diagnostic: Check what files exist before deletion
    ${before_files}=    Run Process    ls    ${WORKLOADS_DIR}    shell=False
    Log    Files before deletion: ${before_files.stdout}

    # Delete workload B using signed deletion
    Delete Workload    workload_b
    Sleep    5s    reason=Wait for persistence plugin to process deletion event

    # Diagnostic: Check files after deletion
    ${after_files}=    Run Process    ls    ${WORKLOADS_DIR}    shell=False
    Log    Files after deletion: ${after_files.stdout}

    # Check plugin logs
    ${plugin_container}=    Run Process    podman    ps    -q    --filter    name\=basic_persistency    shell=False
    ${logs}=    Run Process    podman    logs    --tail    100    ${plugin_container.stdout.strip()}    shell=False
    Log    Plugin logs (last 100 lines): ${logs.stdout}

    # Preserve server logs for analysis
    Run Process    cp    ${TEST_DIR}/ankaios-server.log    /tmp/test7-deletion-server.log    shell=False

    # Verify workload B .pb file was deleted
    File Should Not Exist    ${WORKLOADS_DIR}/workload_b.pb
    ...    msg=Workload B .pb file should be deleted when workload is removed

    # Verify workloads A and C still exist as .pb files
    File Should Exist    ${WORKLOADS_DIR}/workload_a.pb
    File Should Exist    ${WORKLOADS_DIR}/workload_c.pb

    # Delete workload C using signed deletion
    Delete Workload    workload_c
    Sleep    5s    reason=Wait for persistence plugin to process deletion event

    # Verify workload C .pb file was deleted
    File Should Not Exist    ${WORKLOADS_DIR}/workload_c.pb

    # Verify only workload A .pb file remains
    File Should Exist    ${WORKLOADS_DIR}/workload_a.pb
    File Should Not Exist    ${WORKLOADS_DIR}/workload_b.pb
    File Should Not Exist    ${WORKLOADS_DIR}/workload_c.pb

    Log    ✅ Phase 3 complete: Workloads B and C deleted, .pb files removed

    # Phase 4: Server restart - verify only workload A restores
    Log    === PHASE 4: Testing persistence through restart ===

    Stop Ankaios Server
    Sleep    2s

    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${True}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    Sleep    3s    reason=Wait for restoration

    # Verify only workload A was restored
    ${workloads_after_restart}=    Get Workloads
    Should Contain    ${workloads_after_restart}    workload_a
    ...    msg=Workload A should be restored
    Should Not Contain    ${workloads_after_restart}    workload_b
    ...    msg=Workload B should NOT be restored (was deleted)
    Should Not Contain    ${workloads_after_restart}    workload_c
    ...    msg=Workload C should NOT be restored (was deleted)

    # Verify workload A .pb file still exists and has valid signature
    File Should Exist    ${WORKLOADS_DIR}/workload_a.pb
    ${verify_a_final}=    Run Process
    ...    ${ANKAIOS_BIN_DIR}/ank
    ...    verify
    ...    --input    ${WORKLOADS_DIR}/workload_a.pb
    ...    --key    ${KEYS_DIR}/test-key-007.pem.pub
    ...    shell=False
    Should Be Equal As Integers    ${verify_a_final.rc}    0
    ...    msg=Restored workload_a signature should be valid

    # Verify signature verification happened
    ${logs}=    Get Ankaios Server Logs
    Should Contain    ${logs}    signature verified

    Log    ✅ SUCCESS: Complete lifecycle test passed - add, update, delete, and restore work correctly

    [Teardown]    Run Keywords
    ...    Stop Ankaios Server
    ...    AND    Remove File    /tmp/workload_a.yaml
    ...    AND    Remove File    /tmp/workload_a.pb
    ...    AND    Remove File    /tmp/workload_b.yaml
    ...    AND    Remove File    /tmp/workload_b.pb
    ...    AND    Remove File    /tmp/workload_c.yaml
    ...    AND    Remove File    /tmp/workload_c.pb
    ...    AND    Remove Directory    ${WORKLOADS_DIR}    recursive=True

MQTT Signed Manifest Deletion Via Fleet Connector
    [Documentation]    Verify workload persistence via MQTT fleet connector
    ...    NOTE: Python SDK does not yet support signed manifests - this test uses unsigned YAML
    ...    Tests the fleet management flow with persistence:
    ...    1. Fleet sends multiple workload manifests via MQTT (unsigned YAML)
    ...    2. Fleet sends delete request via MQTT
    ...    3. Verify workloads are persisted with persist tag
    ...    4. Verify persistence correctly handles deletions via MQTT
    ...    TODO: Update test to use signed manifests once Python SDK supports signature_metadata
    [Tags]    mqtt    fleet-connector    deletion    persistence

    # Clean state from previous tests
    Run Keyword And Ignore Error    Remove Directory    ${WORKLOADS_DIR}    recursive=True
    Create Directory    ${WORKLOADS_DIR}

    # Setup
    Generate Ed25519 Keypair    test-key-008    ${KEYS_DIR}

    # Note: signature_verification_enabled but require_signature=False
    # This allows the fleet connector (Python SDK) to send unsigned manifests
    # while still allowing signed manifests from CLI
    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${False}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}

    # Start MQTT broker
    # First ensure image is pulled
    ${pull_result}=    Run Process    podman    pull    eclipse-mosquitto:2    shell=False
    Log    Image pull result: ${pull_result.rc}

    # Create safe container names (replace spaces with hyphens)
    ${safe_suite_name}=    Replace String    ${SUITE_NAME}    ${SPACE}    -
    ${mqtt_broker_name}=    Set Variable    mqtt-broker-${safe_suite_name}-${TEST_RUN_ID}
    ${fleet_connector_name}=    Set Variable    fleet-connector-${safe_suite_name}-${TEST_RUN_ID}

    # Clean up any existing containers from previous failed runs
    Run Keyword And Ignore Error    Run Process    podman    stop    ${mqtt_broker_name}    shell=False
    Run Keyword And Ignore Error    Run Process    podman    rm    ${mqtt_broker_name}    shell=False
    Sleep    1s    reason=Wait for cleanup

    ${mqtt_result}=    Run Process    podman    run    -d    --rm    --name    ${mqtt_broker_name}    -p    1883:1883    eclipse-mosquitto:2
    ...    shell=False
    Should Be Equal As Integers    ${mqtt_result.rc}    0    msg=MQTT broker should start: ${mqtt_result.stderr}
    Sleep    2s    reason=Wait for MQTT broker to start

    # Deploy fleet connector as an Ankaios workload (not standalone container)
    # This gives it automatic control interface access
    # Using local build with fixed SDK (delete_manifest now preserves signed_yaml)
    ${fleet_manifest}=    Catenate    SEPARATOR=\n
    ...    apiVersion: v1
    ...    workloads:
    ...    ${SPACE}${SPACE}fleet_connector:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}runtime: podman
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}agent: agent_A
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}runtimeConfig: |
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}image: localhost/fleet-connector:test
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}commandOptions: ["--network=host", "-e", "MQTT_BROKER_ADDR=localhost", "-e", "VIN=test_vehicle_001", "-e", "RUST_LOG=debug"]
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}controlInterfaceAccess:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}allowRules:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}- type: StateRule
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}operation: ReadWrite
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}filterMasks:
    ...    ${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}${SPACE}- "desiredState.workloads.*"

    Create File    /tmp/fleet_connector.yaml    ${fleet_manifest}
    Sign Manifest    /tmp/fleet_connector.yaml    ${KEYS_DIR}/test-key-008.pem    test-key-008    500
    Apply Manifest    /tmp/fleet_connector.pb
    Sleep    5s    reason=Wait for fleet connector workload to start

    # Verify fleet connector is running
    ${workloads}=    Get Workloads
    Should Contain    ${workloads}    fleet_connector    msg=Fleet connector workload should be running

    # Apply first workload via MQTT (unsigned YAML)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/mqtt_workload_1.yaml
    ${content1}=    Get File    /tmp/mqtt_workload_1.yaml
    ${modified1}=    Replace String    ${content1}    nginx-persistent    mqtt_workload_1
    ${modified1}=    Replace String    ${modified1}    persist: ALWAYS    persist: ALWAYS
    ${modified1}=    Replace String    ${modified1}    commandOptions: ["-p", "8080:80"]    commandOptions: []
    Create File    /tmp/mqtt_workload_1.yaml    ${modified1}

    # Publish unsigned YAML via MQTT (Python SDK doesn't support signed manifests yet)
    ${mqtt_content1}=    Get File    /tmp/mqtt_workload_1.yaml
    ${result1}=    Run Process    podman    exec    ${mqtt_broker_name}    mosquitto_pub    -h    localhost    -t    vehicle/test_vehicle_001/manifest/apply/req    -m    ${mqtt_content1}    shell=False
    Should Be Equal As Integers    ${result1.rc}    0    msg=MQTT publish should succeed
    Sleep    3s    reason=Wait for fleet connector to process and apply

    # Preserve server logs for debugging
    Run Process    cp    ${TEST_DIR}/ankaios-server.log    /tmp/test8-mqtt-apply.log    shell=False

    # Check plugin logs
    ${plugin_container}=    Run Process    podman    ps    -q    --filter    name\=basic_persistency    shell=False
    ${plugin_logs}=    Run Process    podman    logs    --tail    100    ${plugin_container.stdout.strip()}    shell=False
    Log    Plugin logs after MQTT apply: ${plugin_logs.stdout}

    # Verify first workload applied and persisted as .yaml file (unsigned)
    File Should Exist    ${WORKLOADS_DIR}/mqtt_workload_1.yaml
    ...    msg=mqtt_workload_1 should be persisted as .yaml file

    # Apply second workload via MQTT (unsigned YAML)
    Copy File    ${FIXTURES_DIR}/signed_workload.yaml    /tmp/mqtt_workload_2.yaml
    ${content2}=    Get File    /tmp/mqtt_workload_2.yaml
    ${modified2}=    Replace String    ${content2}    nginx-persistent    mqtt_workload_2
    ${modified2}=    Replace String    ${modified2}    persist: ALWAYS    persist: ALWAYS
    ${modified2}=    Replace String    ${modified2}    commandOptions: ["-p", "8080:80"]    commandOptions: []
    Create File    /tmp/mqtt_workload_2.yaml    ${modified2}

    ${mqtt_content2}=    Get File    /tmp/mqtt_workload_2.yaml
    ${result2}=    Run Process    podman    exec    ${mqtt_broker_name}    mosquitto_pub    -h    localhost    -t    vehicle/test_vehicle_001/manifest/apply/req    -m    ${mqtt_content2}    shell=False
    Should Be Equal As Integers    ${result2.rc}    0
    Sleep    3s

    # Verify second workload applied and persisted as .yaml file (unsigned)
    File Should Exist    ${WORKLOADS_DIR}/mqtt_workload_2.yaml
    ...    msg=mqtt_workload_2 should be persisted as .yaml file

    # Verify both workloads running
    ${workloads}=    Get Workloads
    Should Contain    ${workloads}    mqtt_workload_1
    Should Contain    ${workloads}    mqtt_workload_2

    # CRITICAL TEST: Delete mqtt_workload_1 via signed MQTT manifest
    Log    === Testing deletion via MQTT ===

    # Create deletion manifest (same workload definition, will be sent to delete topic)
    Copy File    /tmp/mqtt_workload_1.yaml    /tmp/mqtt_delete_1.yaml

    # Publish to DELETE topic (unsigned YAML)
    ${mqtt_delete_content}=    Get File    /tmp/mqtt_delete_1.yaml
    Log    Publishing delete manifest to MQTT (unsigned)
    ${delete_result}=    Run Process    podman    exec    ${mqtt_broker_name}    mosquitto_pub    -h    localhost    -t    vehicle/test_vehicle_001/manifest/delete/req    -m    ${mqtt_delete_content}    shell=False
    Should Be Equal As Integers    ${delete_result.rc}    0    msg=MQTT delete publish should succeed
    Sleep    5s    reason=Wait for deletion to process

    # Check fleet connector logs
    ${fleet_logs}=    Run Process    podman    logs    --tail    50    fleet_connector.${TEST_RUN_ID}    shell=False
    Log    Fleet connector logs: ${fleet_logs.stdout}

    # Verify mqtt_workload_1 was deleted
    ${workloads_after_delete}=    Get Workloads
    Should Not Contain    ${workloads_after_delete}    mqtt_workload_1
    ...    msg=mqtt_workload_1 should be deleted
    Should Contain    ${workloads_after_delete}    mqtt_workload_2
    ...    msg=mqtt_workload_2 should still exist

    # Verify persistence .yaml file was removed
    File Should Not Exist    ${WORKLOADS_DIR}/mqtt_workload_1.yaml
    ...    msg=Deleted workload .yaml file should be removed from persistence
    File Should Exist    ${WORKLOADS_DIR}/mqtt_workload_2.yaml
    ...    msg=Non-deleted workload .yaml file should remain

    # Test server restart - only mqtt_workload_2 should restore
    Log    === Testing persistence after deletion ===

    Stop Ankaios Server
    Sleep    2s

    Start Ankaios Server
    ...    signature_verification_enabled=${True}
    ...    require_signature=${False}
    ...    keys_dir=${KEYS_DIR}
    ...    persistence_plugin=${True}
    ...    workloads_dir=${WORKLOADS_DIR}
    Sleep    3s

    # Verify only mqtt_workload_2 restored
    ${workloads_restored}=    Get Workloads
    Should Not Contain    ${workloads_restored}    mqtt_workload_1
    ...    msg=Deleted workload should NOT restore
    Should Contain    ${workloads_restored}    mqtt_workload_2
    ...    msg=Non-deleted workload should restore

    Log    ✅ SUCCESS: MQTT deletion flow works - persistence handles deletion correctly

    [Teardown]    Run Keywords
    ...    Run Process    podman    stop    ${mqtt_broker_name}    shell=False
    ...    AND    Stop Ankaios Server
    ...    AND    Remove File    /tmp/mqtt_workload_1.yaml
    ...    AND    Remove File    /tmp/mqtt_workload_2.yaml
    ...    AND    Remove File    /tmp/mqtt_delete_1.yaml
    ...    AND    Remove File    /tmp/fleet_connector.yaml
    ...    AND    Remove File    /tmp/fleet_connector.pb
    ...    AND    Remove Directory    ${WORKLOADS_DIR}    recursive=True
